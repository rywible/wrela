// Shared by trace.wgsl and light.wgsl: the frame uniform, hash, terrain, masks, trees, crown envelopes
// and wind. world.js mirrors the world functions (hash, terrain, masks, tree_at, envelopes) exactly.

struct Frame {
  eye: vec4f,       // xyz camera; w: a pixel's angular size (radians per pixel)
  cf: vec4f,        // camera forward; w: time (s)
  cr: vec4f,        // camera right × tan(half fov x); w: wind strength (0 = still)
  cu: vec4f,        // camera up × tan(half fov y); w: frame counter (stochastic choices)
  sun: vec4f,       // xyz toward the sun; w: the sun's angular radius (rad)
  jit: vec4f,       // xy: sub-pixel jitter (pixels); z: view distance (m); w: TAA blend floor
  peye: vec4f,      // previous frame's camera (reprojection)
  pcf: vec4f,
  pcr: vec4f,
  pcu: vec4f,
  world: vec4f,     // x: tree occupancy, y: grass blades per cell, z: fern density, w: TAA frames since reset
  lod: vec4f,       // x: k0 (leaf→volume, footprint in leaf cells), y: k1 (volume→far, footprint in m), z: min level, w: forced level (−1 auto)
  lod2: vec4f,      // x: grass k (footprint in blade widths), y: fern k (in frond widths), z: volume steps per crown, w: shadow volume steps
  iso: vec4i,       // xy: isolated tree cell, z: 1 = only that tree, w: 1 = no floor (grass, ferns)
  dims: vec4u,      // width, height, frame index, reference sample index
  calib: array<vec4f, 12>,   // c[species][mip][band], 2 × 8 × 3
  far: vec4f,       // x, y: far extinction (m⁻¹) broadleaf, conifer; z, w: far shape scale
  org: vec4u,       // the tile this dispatch covers: origin x, y and size w, h (long renders are tiled)
  dbg: vec4u,       // x: feature-off bits, for the cost breakdown (DBG_*); 0 in every measured configuration
  lod3: vec4f,      // x: explicit-leaf cell budget per crown before the rest becomes volume; y, z: far extinction 34° down
}

const DBG_GRASS: u32 = 1u;
const DBG_FERN: u32 = 2u;
const DBG_UNDER: u32 = 4u;
const DBG_BARK: u32 = 8u;
const DBG_CROWN: u32 = 16u;
const DBG_SHADOW_TREES: u32 = 32u;
const DBG_AO: u32 = 64u;
fn dbg(bit: u32) -> bool { return (F.dbg.x & bit) != 0u; }

/** The pixel a trace or light invocation covers, and whether it's inside the frame and the tile. */
fn tile_px(gid: vec3u) -> vec3u {
  let p = gid.xy + F.org.xy;
  let ok = all(gid.xy < F.org.zw) && all(p < F.dims.xy);
  return vec3u(p, select(0u, 1u, ok));
}

@group(0) @binding(0) var<uniform> F: Frame;

const INF: f32 = 1e30;
const PI: f32 = 3.14159265;

// ---- hash ----------------------------------------------------------------------------------------------

fn pcg3(v0: vec3u) -> vec3u {
  var v = v0 * 1664525u + vec3u(1013904223u);
  v.x += v.y * v.z; v.y += v.z * v.x; v.z += v.x * v.y;
  v ^= v >> vec3u(16u);
  v.x += v.y * v.z; v.y += v.z * v.x; v.z += v.x * v.y;
  return v;
}
fn u01(x: u32) -> f32 { return f32(x >> 8u) * (1.0 / 16777216.0); }
fn u01v(v: vec3u) -> vec3f { return vec3f(v >> vec3u(8u)) * (1.0 / 16777216.0); }

/** Per-pixel, per-frame random number, salted. Drives stochastic LOD choices and march phases. */
fn prand(px: vec2u, salt: u32) -> f32 {
  return u01(pcg3(vec3u(px.x, px.y, F.dims.z * 7919u + F.dims.w * 104729u + salt)).x);
}

// ---- terrain ------------------------------------------------------------------------------------------------

const T_SLOPE: f32 = 0.32;      // sup |∇h| ≤ 0.313 (sum of each term's amplitude × frequency norm)
const T_HMAX: f32 = 11.6;       // |h| ≤ 11.55
const CANOPY: f32 = 28.5;       // highest crown point above the terrain (conifer ≤ 28 m above a base 0.2 m down)
const SHELL: f32 = 0.8;         // the floor layer: grass and ferns live below terrain + SHELL

fn terrain_h(x: f32, z: f32) -> f32 {
  return 7.0 * sin(0.0071 * x + 0.4) * cos(0.0059 * z - 0.3)
       + 3.2 * sin(0.017 * x - 0.013 * z + 1.7) * cos(0.021 * z + 0.6)
       + 1.1 * sin(0.043 * x + 2.1) * sin(0.051 * z + 0.9)
       + 0.25 * sin(0.13 * x + 0.17 * z);
}

/** Height and gradient: (h, ∂h/∂x, ∂h/∂z). */
fn terrain_hg(x: f32, z: f32) -> vec3f {
  let a1 = 0.0071 * x + 0.4; let b1 = 0.0059 * z - 0.3;
  let a2 = 0.017 * x - 0.013 * z + 1.7; let b2 = 0.021 * z + 0.6;
  let a3 = 0.043 * x + 2.1; let b3 = 0.051 * z + 0.9;
  let a4 = 0.13 * x + 0.17 * z;
  let s1 = sin(a1); let c1 = cos(a1); let s1b = sin(b1); let c1b = cos(b1);
  let s2 = sin(a2); let c2 = cos(a2); let s2b = sin(b2); let c2b = cos(b2);
  let s3 = sin(a3); let c3 = cos(a3); let s3b = sin(b3); let c3b = cos(b3);
  let s4 = sin(a4); let c4 = cos(a4);
  let h = 7.0 * s1 * c1b + 3.2 * s2 * c2b + 1.1 * s3 * s3b + 0.25 * s4;
  let hx = 7.0 * 0.0071 * c1 * c1b + 3.2 * 0.017 * c2 * c2b + 1.1 * 0.043 * c3 * s3b + 0.25 * 0.13 * c4;
  let hz = -7.0 * 0.0059 * s1 * s1b + 3.2 * (-0.013 * c2 * c2b - 0.021 * s2 * s2b) + 1.1 * 0.051 * s3 * c3b + 0.25 * 0.17 * c4;
  return vec3f(h, hx, hz);
}

fn terrain_n(x: f32, z: f32) -> vec3f {
  let g = terrain_hg(x, z);
  return normalize(vec3f(-g.y, 1.0, -g.z));
}

// ---- masks -------------------------------------------------------------------------------------------------------

const CLEAR_R: f32 = 22.0;

fn meadow_field(x: f32, z: f32) -> f32 {
  return sin(0.0123 * x + 1.3) * sin(0.0151 * z + 0.2) + 0.5 * sin(0.031 * x - 0.027 * z + 0.4);
}
fn forest_mask(x: f32, z: f32) -> f32 {
  let r = length(vec2f(x, z));
  return smoothstep(CLEAR_R, CLEAR_R + 9.0, r) * (1.0 - smoothstep(0.95, 1.25, meadow_field(x, z)));
}
fn conifer_share(x: f32, z: f32) -> f32 {
  let s = 0.4 + 0.45 * sin(0.0093 * x + 0.3) * cos(0.0117 * z - 1.2) + 0.15 * sin(0.041 * x + 0.033 * z);
  return clamp(s, 0.05, 0.92);
}
/** Grass presence (0–1): the clearing and the meadows. Lipschitz ≤ 0.12 per metre. */
fn grass_mask(x: f32, z: f32) -> f32 {
  let r = length(vec2f(x, z));
  let clearing = 1.0 - smoothstep(CLEAR_R - 3.0, CLEAR_R + 7.0, r);
  let meadow = smoothstep(0.85, 1.15, meadow_field(x, z));
  return max(clearing, meadow);
}
/** Fern presence (0–1): a band at the forest's edge, and patches under the trees. */
fn fern_mask(x: f32, z: f32) -> f32 {
  let r = length(vec2f(x, z));
  let edge = smoothstep(CLEAR_R - 5.0, CLEAR_R + 1.0, r) * (1.0 - smoothstep(CLEAR_R + 18.0, CLEAR_R + 45.0, r));
  let patches = smoothstep(0.25, 0.75, sin(0.11 * x + 0.7) * sin(0.13 * z + 1.9) + 0.4 * sin(0.05 * x - 0.07 * z));
  return max(0.85 * edge, 0.6 * patches) * (1.0 - smoothstep(0.95, 1.2, meadow_field(x, z)));
}

// ---- trees: constants and the hash (the per-tree functions are in foliage.wgsl) ------------------------------------

const TREE_C: f32 = 6.0;        // grid cell (m)
const TREE_J: f32 = 1.5;        // trunk jitter ±
const TREE_OV: f32 = 3.5;       // crowns reach at most this far past their cell (jitter 1.5 + bound 4.6 + sway 0.32 − C/2)
const SWAY_MAX: f32 = 0.32;     // |sway| at wind strength 1
const UNDER_OCC: f32 = 0.35;    // understory occupancy, of the forest mask
const MAP_HALF: i32 = 768;      // the cooked tree table covers cells −768…767 on each axis (±4.6 km)
const WIND_DIR = vec2f(0.958, 0.287);

/** Slot 0 is the cell's canopy tree, slot 1 its understory tree. */
fn tree_hash(ci: vec2i, slot: u32) -> vec3u {
  return pcg3(vec3u(bitcast<u32>(ci.x), bitcast<u32>(ci.y), 0x2545F491u + slot * 0x9E3779B9u));
}
fn tree_pos(ci: vec2i, h: vec3u) -> vec2f {
  return (vec2f(ci) + 0.5) * TREE_C + (vec2f(u01(h.y), u01(h.z)) - 0.5) * 2.0 * TREE_J;
}
fn tree_occ(slot: u32) -> f32 { return select(F.world.x, UNDER_OCC, slot == 1u); }

/** Conservative horizontal bound of anything slot `slot` can draw, before its shape is known. */
fn slot_bound(slot: u32) -> f32 { return select(4.6, 2.5, slot == 1u) + SWAY_MAX; }

// ---- species ---------------------------------------------------------------------------------------------------------

const LEAF_K: u32 = 6u;         // leaves per cell, per grid
const LEAF_U32: u32 = 20u;      // packed words per cell: head + 3 per leaf + pad (world.js leafTable)
const LEAF_P: u32 = 16u;        // tile period in cells
const LEAF_P3: u32 = 4096u;
fn leaf_cell_m(kind: u32) -> f32 { return select(0.22, 0.10, kind == 1u); }
/** Typical leaf width (m): what the explicit→volume transition compares the footprint with. */
fn leaf_width_m(kind: u32) -> f32 { return select(0.22 * 2.0 * 0.165 * 0.62, 0.10 * 2.0 * 0.225 * 0.30, kind == 1u); }

// ---- intersections -------------------------------------------------------------------------------------------------------

/** Ray against a vertical circle in xz: the t interval where the ray's xz lies inside. */
fn circle_xz(o: vec3f, d: vec3f, c: vec2f, r: f32) -> vec2f {
  let p = o.xz - c;
  let a = dot(d.xz, d.xz);
  if (a < 1e-12) { return select(vec2f(1.0, -1.0), vec2f(-INF, INF), dot(p, p) <= r * r); }
  let b = dot(p, d.xz);
  let cc = dot(p, p) - r * r;
  let disc = b * b - a * cc;
  if (disc < 0.0) { return vec2f(1.0, -1.0); }
  let s = sqrt(disc);
  return vec2f((-b - s) / a, (-b + s) / a);
}

/** Ray against an axis-aligned ellipsoid at c with radii r: the chord interval. */
fn ellipsoid_iv(o: vec3f, d: vec3f, c: vec3f, r: vec3f) -> vec2f {
  let oo = (o - c) / r;
  let dd = d / r;
  let a = dot(dd, dd);
  let b = dot(oo, dd);
  let cc = dot(oo, oo) - 1.0;
  let disc = b * b - a * cc;
  if (disc < 0.0) { return vec2f(1.0, -1.0); }
  let s = sqrt(disc);
  return vec2f((-b - s) / a, (-b + s) / a);
}

/** Ray against a solid vertical cone: base centre b, base radius rb, height hc (apex above). */
fn cone_iv(o: vec3f, d: vec3f, b: vec3f, rb: f32, hc: f32) -> vec2f {
  let apex = b + vec3f(0.0, hc, 0.0);
  var lo = -INF;
  var hi = INF;
  if (abs(d.y) > 1e-9) {
    let ta = (b.y - o.y) / d.y;
    let tb = (apex.y - o.y) / d.y;
    lo = min(ta, tb); hi = max(ta, tb);
  } else if (o.y < b.y || o.y > apex.y) { return vec2f(1.0, -1.0); }
  let k = rb / hc;
  let k2 = k * k;
  let p = o - apex;
  let a = d.x * d.x + d.z * d.z - k2 * d.y * d.y;
  let bb = p.x * d.x + p.z * d.z - k2 * p.y * d.y;
  let c = p.x * p.x + p.z * p.z - k2 * p.y * p.y;
  let disc = bb * bb - a * c;
  if (disc < 0.0 || abs(a) < 1e-12) { return vec2f(1.0, -1.0); }
  let s = sqrt(disc);
  let r1 = (-bb - s) / a;
  let r2 = (-bb + s) / a;
  let rl = min(r1, r2);
  let rh = max(r1, r2);
  if (a > 0.0) {
    if (p.y + 0.5 * (rl + rh) * d.y > 0.0) { return vec2f(1.0, -1.0); }   // the upper nappe
    lo = max(lo, rl); hi = min(hi, rh);
  } else {
    if (p.y + (rl - 1.0) * d.y <= 0.0) { hi = min(hi, rl); } else { lo = max(lo, rh); }
  }
  return vec2f(lo, hi);
}

/** Ray against a vertical frustum's side (radius r0 at y0, r1 at y1): nearest t > tmin, and the normal. */
fn frustum_hit(o: vec3f, d: vec3f, c: vec2f, y0: f32, y1: f32, r0: f32, r1: f32, tmin: f32) -> vec4f {
  let k = (r1 - r0) / (y1 - y0);
  let p = vec3f(o.x - c.x, o.y - y0, o.z - c.y);
  let rr = r0 + k * p.y;
  let a = d.x * d.x + d.z * d.z - k * k * d.y * d.y;
  let b = p.x * d.x + p.z * d.z - k * rr * d.y;
  let cc = p.x * p.x + p.z * p.z - rr * rr;
  let disc = b * b - a * cc;
  if (disc < 0.0 || abs(a) < 1e-12) { return vec4f(INF); }
  let s = sqrt(disc);
  var t = (-b - s) / a;
  if (t < tmin) { t = (-b + s) / a; }
  if (t < tmin) { return vec4f(INF); }
  let q = p + d * t;
  if (q.y < 0.0 || q.y > y1 - y0) { return vec4f(INF); }
  let r = r0 + k * q.y;
  if (r <= 0.0) { return vec4f(INF); }
  return vec4f(t, normalize(vec3f(q.x, -k * r, q.z)));
}

/** Ray against a capsule (iq): nearest t > tmin and the normal. */
fn capsule_hit(o: vec3f, d: vec3f, pa: vec3f, pb: vec3f, r: f32, tmin: f32) -> vec4f {
  let ba = pb - pa;
  let oa = o - pa;
  let baba = dot(ba, ba);
  let bard = dot(ba, d);
  let baoa = dot(ba, oa);
  let rdoa = dot(d, oa);
  let oaoa = dot(oa, oa);
  let a = baba - bard * bard;
  let b = baba * rdoa - baoa * bard;
  let c = baba * oaoa - baoa * baoa - r * r * baba;
  let h = b * b - a * c;
  if (h < 0.0) { return vec4f(INF); }
  var t = (-b - sqrt(h)) / a;
  let y = baoa + t * bard;
  if (y > 0.0 && y < baba) {
    if (t < tmin) { return vec4f(INF); }
    let q = oa + d * t;
    return vec4f(t, (q - ba * (y / baba)) / r);
  }
  let oc = select(o - pb, oa, y <= 0.0);
  let cap = select(pb, pa, y <= 0.0);
  let b2 = dot(d, oc);
  let c2 = dot(oc, oc) - r * r;
  let h2 = b2 * b2 - c2;
  if (h2 <= 0.0) { return vec4f(INF); }
  t = -b2 - sqrt(h2);
  if (t < tmin) { return vec4f(INF); }
  return vec4f(t, normalize(o + d * t - cap));
}

// ---- camera -------------------------------------------------------------------------------------------------------

fn ray_dir(pix: vec2f) -> vec3f {
  let u = pix.x / f32(F.dims.x) * 2.0 - 1.0;
  let v = 1.0 - pix.y / f32(F.dims.y) * 2.0;
  return normalize(F.cf.xyz + F.cr.xyz * u + F.cu.xyz * v);
}

// ---- G-buffer packing ----------------------------------------------------------------------------------------------

const MAT_SKY: u32 = 0u;
const MAT_GROUND: u32 = 1u;
const MAT_BARK: u32 = 2u;
const MAT_LEAF: u32 = 3u;
const MAT_NEEDLE: u32 = 4u;
const MAT_GRASS: u32 = 5u;
const MAT_FERN: u32 = 6u;
const MAT_DEBUG: u32 = 7u;

fn oct_enc(n: vec3f) -> u32 {
  var p = n.xy / (abs(n.x) + abs(n.y) + abs(n.z));
  if (n.z < 0.0) { p = (1.0 - abs(p.yx)) * select(vec2f(-1.0), vec2f(1.0), p >= vec2f(0.0)); }
  let q = vec2u(clamp(p * 0.5 + 0.5, vec2f(0.0), vec2f(1.0)) * 4095.0 + 0.5);
  return q.x | (q.y << 12u);
}
fn oct_dec(e: u32) -> vec3f {
  let p = vec2f(f32(e & 4095u), f32((e >> 12u) & 4095u)) / 4095.0 * 2.0 - 1.0;
  var n = vec3f(p, 1.0 - abs(p.x) - abs(p.y));
  if (n.z < 0.0) { n = vec3f((1.0 - abs(n.yx)) * select(vec2f(-1.0), vec2f(1.0), n.xy >= vec2f(0.0)), n.z); }
  return normalize(n);
}
// Albedo is stored as sqrt in 8 bits, so dark foliage doesn't band.
fn alb_enc(a: vec3f, w: f32) -> u32 { return pack4x8unorm(vec4f(sqrt(clamp(a, vec3f(0.0), vec3f(1.0))), w)); }
fn alb_dec(e: u32) -> vec4f { let v = unpack4x8unorm(e); return vec4f(v.rgb * v.rgb, v.a); }
