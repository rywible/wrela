// Shared by every pass: the frame uniform, SDF helpers (fieldview's library, copied), hashing,
// noise and the G-buffer packing.

struct Frame {
  eye: vec4f,     // xyz camera; w: a pixel's angular size (2 tan(fovy/2) / height)
  cf: vec4f,      // forward
  cr: vec4f,      // right × tan(half fov x)
  cu: vec4f,      // up × tan(half fov y)
  sun: vec4f,     // xyz: toward the sun
  prm: vec4f,     // x: line width px, y: stroke cell px, z: Kuwahara radius px, w: crease width px
  prm2: vec4f,    // x: halo reach px, y: resolution scale (1 at 1080p), z, w: unused
  wolf: vec4f,    // xyz: the wolf's root, w: yaw
  dims: vec4u,    // width, height, tile origin x, y (a reference frame renders in tiles)
  aeye: vec4f,    // camera A (flicker measure only): the same four vectors
  acf: vec4f,
  acr: vec4f,
  acu: vec4f,
}

@group(0) @binding(0) var<uniform> F: Frame;

const INF: f32 = 1e30;
const PI: f32 = 3.14159265;

fn ray_dir(px: vec2u) -> vec3f {
  let u = (f32(px.x) + 0.5) / f32(F.dims.x) * 2.0 - 1.0;
  let v = 1.0 - (f32(px.y) + 0.5) / f32(F.dims.y) * 2.0;
  return normalize(F.cf.xyz + F.cr.xyz * u + F.cu.xyz * v);
}

// ---- G-buffer: rgba32uint ---------------------------------------------------------------------
//   x: hit distance t (f32 bits; INF for sky)
//   y: normal, octahedral, 2×16 snorm
//   z: bits 0–3 material, 4–7 line material, 8–15 steps (saturated), 16–31 line distance px × 1024 (0–64 px)
//   w: line distance along the ray (f32 bits)

fn oct_wrap(v: vec2f) -> vec2f {
  return (1.0 - abs(v.yx)) * select(vec2f(-1.0), vec2f(1.0), v >= vec2f(0.0));
}
fn oct_enc(n: vec3f) -> u32 {
  var p = n.xy / (abs(n.x) + abs(n.y) + abs(n.z));
  if (n.z < 0.0) { p = oct_wrap(p); }
  return pack2x16snorm(p);
}
fn oct_dec(u: u32) -> vec3f {
  let f = unpack2x16snorm(u);
  var n = vec3f(f, 1.0 - abs(f.x) - abs(f.y));
  let t = max(-n.z, 0.0);
  n.x += select(t, -t, n.x >= 0.0);
  n.y += select(t, -t, n.y >= 0.0);
  return normalize(n);
}

struct GB { t: f32, n: vec3f, mat: u32, line_mat: u32, steps: u32, line_px: f32, line_t: f32 }

fn gb_pack(g: GB) -> vec4u {
  let lq = u32(clamp(g.line_px, 0.0, 63.99) * 1024.0);
  return vec4u(bitcast<u32>(g.t), oct_enc(g.n),
               (g.mat & 15u) | ((g.line_mat & 15u) << 4u) | (min(g.steps, 255u) << 8u) | (lq << 16u),
               bitcast<u32>(g.line_t));
}
fn gb_unpack(v: vec4u) -> GB {
  var g: GB;
  g.t = bitcast<f32>(v.x);
  g.n = oct_dec(v.y);
  g.mat = v.z & 15u;
  g.line_mat = (v.z >> 4u) & 15u;
  g.steps = (v.z >> 8u) & 255u;
  g.line_px = f32(v.z >> 16u) / 1024.0;
  g.line_t = bitcast<f32>(v.w);
  return g;
}

// ---- hashing ----------------------------------------------------------------------------------

// Jarzynski & Olano 2020, "Hash Functions for GPU Rendering".
fn pcg4d(v0: vec4u) -> vec4u {
  var v = v0 * 1664525u + 1013904223u;
  v.x += v.y * v.w; v.y += v.z * v.x; v.z += v.x * v.y; v.w += v.y * v.z;
  v ^= v >> vec4u(16u);
  v.x += v.y * v.w; v.y += v.z * v.x; v.z += v.x * v.y; v.w += v.y * v.z;
  return v;
}

// ---- fieldview's helper library (experiments/agent-authoring/fieldview/src/lib.wgsl), copied ----

fn sd_sphere(p: vec3f, r: f32) -> f32 {
  return length(p) - r;
}

fn sd_box(p: vec3f, half: vec3f) -> f32 {
  let q = abs(p) - half;
  return length(max(q, vec3f(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn sd_round_cone(p: vec3f, a: vec3f, b: vec3f, r1: f32, r2: f32) -> f32 {
  let ba = b - a;
  let l2 = dot(ba, ba);
  let rr = r1 - r2;
  let a2 = l2 - rr * rr;
  let il2 = 1.0 / l2;
  let pa = p - a;
  let y = dot(pa, ba);
  let z = y - l2;
  let xv = pa * l2 - ba * y;
  let x2 = dot(xv, xv);
  let y2 = y * y * l2;
  let z2 = z * z * l2;
  let k = sign(rr) * rr * rr * x2;
  if (sign(z) * a2 * z2 > k) { return sqrt(x2 + z2) * il2 - r2; }
  if (sign(y) * a2 * y2 < k) { return sqrt(x2 + y2) * il2 - r1; }
  return (sqrt(x2 * a2 * il2) + y * rr) * il2 - r1;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
  let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
  return mix(b, a, h) - k * h * (1.0 - h);
}

fn smax(a: f32, b: f32, k: f32) -> f32 {
  return -smin(-a, -b, k);
}

fn rot_x(p: vec3f, a: f32) -> vec3f { let c = cos(a); let s = sin(a); return vec3f(p.x, c * p.y - s * p.z, s * p.y + c * p.z); }
fn rot_y(p: vec3f, a: f32) -> vec3f { let c = cos(a); let s = sin(a); return vec3f(c * p.x + s * p.z, p.y, -s * p.x + c * p.z); }
fn rot_z(p: vec3f, a: f32) -> vec3f { let c = cos(a); let s = sin(a); return vec3f(c * p.x - s * p.y, s * p.x + c * p.y, p.z); }
fn mirror_x(p: vec3f) -> vec3f { return vec3f(abs(p.x), p.y, p.z); }

fn fv_hash(c: vec3i) -> f32 {
  var h = bitcast<u32>(c.x) * 747796405u + bitcast<u32>(c.y) * 2891336453u + bitcast<u32>(c.z) * 277803737u;
  h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
  h = (h >> 22u) ^ h;
  return f32(h) * (2.0 / 4294967295.0) - 1.0;
}

// Value noise in [-1, 1]. Its gradient is at most 3 per unit of frequency along an axis, 3√3 ≈ 5.2
// in magnitude (strict); the canopy's step scales are derived from that.
fn noise3(x: vec3f) -> f32 {
  let fl = floor(x);
  let i = vec3i(fl);
  let f = x - fl;
  let u = f * f * (3.0 - 2.0 * f);
  return mix(mix(mix(fv_hash(i), fv_hash(i + vec3i(1, 0, 0)), u.x),
                 mix(fv_hash(i + vec3i(0, 1, 0)), fv_hash(i + vec3i(1, 1, 0)), u.x), u.y),
             mix(mix(fv_hash(i + vec3i(0, 0, 1)), fv_hash(i + vec3i(1, 0, 1)), u.x),
                 mix(fv_hash(i + vec3i(0, 1, 1)), fv_hash(i + vec3i(1, 1, 1)), u.x), u.y), u.z);
}

fn fbm3(x: vec3f, octaves: i32) -> f32 {
  var s = 0.0;
  var a = 0.5;
  var q = x;
  for (var o = 0; o < octaves; o++) {
    s += a * noise3(q);
    q = q * 2.03 + vec3f(1.7, 9.2, 3.1);
    a *= 0.5;
  }
  return s;
}

// ---- closest approach between two march samples -------------------------------------------------

/** Quílez's closest-approach estimate, generalized to any step: samples ds apart along the ray with
 *  distances ph (previous) and h (current). The nearest surface point is assumed to lie on both
 *  spheres. Returns (distance from the ray, how far back from the current sample). When the foot
 *  falls outside the step (or the samples aren't consistent), it falls back to the current sample. */
fn closest_approach(ph: f32, h: f32, ds: f32) -> vec2f {
  // Only for sphere-tracing-like steps (no longer than the previous distance). A longer step (the
  // terrain's directional step) can put the spheres nearly tangent, and the estimate then invents a
  // narrow waist between them. One sphere inside the other bounds nothing new.
  if (ph >= INF || ds <= abs(ph - h) || ds > ph * 1.001) { return vec2f(h, 0.0); }
  let x = (ph * ph - h * h + ds * ds) / (2.0 * ds);
  if (x <= 0.0 || x >= ds) { return vec2f(h, 0.0); }
  return vec2f(sqrt(max(ph * ph - x * x, 0.0)), ds - x);
}

// ---- colour ---------------------------------------------------------------------------------------

fn srgb(c: vec3f) -> vec3f { return pow(c, vec3f(2.2)); }      // display value → linear
fn luma(c: vec3f) -> f32 { return dot(c, vec3f(0.2126, 0.7152, 0.0722)); }

fn tonemap(x: vec3f) -> vec3f {   // ACES fit (Narkowicz), as spikes 01 and 02
  let c = x * 0.8;
  return clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), vec3f(0.0), vec3f(1.0));
}
