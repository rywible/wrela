// Shared by every scene module: the frame uniform, noise, SDF helpers, sky, BRDF and tonemap.
// A module is common.wgsl + [wolf.wgsl] + scene_<name>.wgsl + render.wgsl (main.js assembles it).

struct Frame {
  eye: vec4f,       // xyz camera; w: a pixel's angular size at the current resolution (radians at distance 1)
  cf: vec4f,        // camera forward
  cr: vec4f,        // camera right × tan(half fov x)
  cu: vec4f,        // camera up × tan(half fov y)
  sun: vec4f,       // xyz: direction toward the sun
  sunc: vec4f,      // rgb: sun irradiance; w: exposure
  dims: vec4u,      // width, height, first and end row of this dispatch (tiled references)
  prm: vec4f,       // x: footprint scale (sweep), y: time, z: wetness, w: unused
}

@group(0) @binding(0) var<uniform> F: Frame;

const PI: f32 = 3.14159265;
const INF: f32 = 1e30;
const SALT: f32 = 0.0;      // replaced per run when measuring cold pipeline creation

// Material ids, written by the visibility pass.
const M_SKY: u32 = 0u;
const M_GROUND: u32 = 1u;
const M_HIDE: u32 = 2u;     // skin, hide, membranes: translucent
const M_LEAF: u32 = 3u;     // translucent
const M_EYE: u32 = 4u;
const M_STONE: u32 = 5u;    // the layered material applies
const M_MORTAR: u32 = 6u;   // the layered material applies
const M_WOOD: u32 = 7u;
const M_GLOSS: u32 = 8u;    // the hammered glossy sphere
const M_NOSE: u32 = 9u;
const M_SLAB: u32 = 10u;    // the gallery's wet slab
const N_IDS: u32 = 11u;

// Material switches (the MATS and REF overrides in render.wgsl).
const B_SKIN: u32 = 1u;
const B_LEAF: u32 = 2u;
const B_EYE: u32 = 4u;
const B_WET: u32 = 8u;
const B_LAYER: u32 = 16u;
const B_SPECAA: u32 = 32u;

// ---- hashing and noise ------------------------------------------------------------------------------

fn hash_i3(c: vec3i) -> f32 {        // fieldview's hash, in [-1, 1]
  var h = bitcast<u32>(c.x) * 747796405u + bitcast<u32>(c.y) * 2891336453u + bitcast<u32>(c.z) * 277803737u;
  h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
  h = (h >> 22u) ^ h;
  return f32(h) * (2.0 / 4294967295.0) - 1.0;
}

fn hash_u(n: u32) -> f32 {           // [0, 1)
  var h = n * 747796405u + 2891336453u;
  h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
  h = (h >> 22u) ^ h;
  return f32(h) * (1.0 / 4294967296.0);
}

fn hash2u(a: i32, b: i32) -> f32 { return 0.5 + 0.5 * hash_i3(vec3i(a, b, 17)); }

/** Value noise in [-1, 1]. */
fn noise3(x: vec3f) -> f32 {
  let fl = floor(x);
  let i = vec3i(fl);
  let f = x - fl;
  let u = f * f * (3.0 - 2.0 * f);
  return mix(mix(mix(hash_i3(i), hash_i3(i + vec3i(1, 0, 0)), u.x),
                 mix(hash_i3(i + vec3i(0, 1, 0)), hash_i3(i + vec3i(1, 1, 0)), u.x), u.y),
             mix(mix(hash_i3(i + vec3i(0, 0, 1)), hash_i3(i + vec3i(1, 0, 1)), u.x),
                 mix(hash_i3(i + vec3i(0, 1, 1)), hash_i3(i + vec3i(1, 1, 1)), u.x), u.y), u.z);
}

/** The same noise with its analytic gradient: (value, ∂x, ∂y, ∂z). */
fn noised(x: vec3f) -> vec4f {
  let fl = floor(x);
  let i = vec3i(fl);
  let f = x - fl;
  let u = f * f * (3.0 - 2.0 * f);
  let du = 6.0 * f * (1.0 - f);
  let a = hash_i3(i);
  let b = hash_i3(i + vec3i(1, 0, 0));
  let c = hash_i3(i + vec3i(0, 1, 0));
  let d = hash_i3(i + vec3i(1, 1, 0));
  let e = hash_i3(i + vec3i(0, 0, 1));
  let g = hash_i3(i + vec3i(1, 0, 1));
  let h = hash_i3(i + vec3i(0, 1, 1));
  let k = hash_i3(i + vec3i(1, 1, 1));
  let k1 = b - a;
  let k2 = c - a;
  let k3 = e - a;
  let k4 = a - b - c + d;
  let k5 = a - c - e + h;
  let k6 = a - b - e + g;
  let k7 = -a + b + c - d + e - g - h + k;
  let v = a + k1 * u.x + k2 * u.y + k3 * u.z + k4 * u.x * u.y + k5 * u.y * u.z + k6 * u.z * u.x + k7 * u.x * u.y * u.z;
  let gr = du * vec3f(k1 + k4 * u.y + k6 * u.z + k7 * u.y * u.z,
                      k2 + k5 * u.z + k4 * u.x + k7 * u.z * u.x,
                      k3 + k6 * u.x + k5 * u.y + k7 * u.x * u.y);
  return vec4f(v, gr);
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

/** Octave weight under footprint filtering (D-077): an octave of frequency f (cycles per metre) fades
 *  out between 4 and 2 samples per wavelength, for a pixel footprint fp (metres). */
fn octave_w(f: f32, fp: f32) -> f32 {
  if (!FILTER) { return 1.0; }
  return 1.0 - smoothstep(0.25, 0.5, f * fp);
}

/** fbm whose octaves fade with the footprint; value only. For albedo and mask noise. */
fn fbm_f(x: vec3f, f0: f32, octaves: i32, fp: f32) -> f32 {
  var s = 0.0;
  var a = 0.5;
  var f = f0;
  var q = x * f0;
  for (var o = 0; o < octaves; o++) {
    let w = octave_w(f, fp);
    if (w <= 0.0) { break; }
    s += a * w * noise3(q);
    q = q * 2.03 + vec3f(1.7, 9.2, 3.1);
    f *= 2.03;
    a *= 0.5;
  }
  return s;
}

// The noise's declared slope fact: E|∇n|² = 1.32 per unit frequency (measured once offline over 2M
// samples of this exact noise). Two thirds of it is tangential to a surface, for isotropic noise.
const NOISE_G2_TAN: f32 = 0.88;

/** Surface detail: a displacement h of `octaves` octaves of noise, applied only to the shading
 *  normal. Returns h, ∇h of the octaves the footprint resolves, and the slope variance of the ones it
 *  fades out (what specular AA adds to roughness²). */
struct Det { v: f32, g: vec3f, var_: f32 }

fn detail(p: vec3f, f0: f32, a0: f32, octaves: i32, fp: f32) -> Det {
  var r = Det(0.0, vec3f(0.0), 0.0);
  var f = f0;
  var a = a0;
  for (var o = 0; o < octaves; o++) {
    let w = octave_w(f, fp);
    let slope2 = a * a * f * f * NOISE_G2_TAN;
    r.var_ += (1.0 - w * w) * slope2;
    if (w > 0.0) {
      let n = noised(p * f + vec3f(f32(o) * 7.31, f32(o) * 3.17, f32(o) * 5.53));
      r.v += a * w * n.x;
      r.g += a * f * w * n.yzw;
    }
    f *= 2.03;
    a *= 0.5;
  }
  return r;
}

// ---- SDF helpers (fieldview's lib.wgsl, copied) --------------------------------------------------------

fn sd_sphere(p: vec3f, r: f32) -> f32 { return length(p) - r; }

fn sd_box(p: vec3f, half: vec3f) -> f32 {
  let q = abs(p) - half;
  return length(max(q, vec3f(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn sd_round_box(p: vec3f, half: vec3f, r: f32) -> f32 { return sd_box(p, half - vec3f(r)) - r; }

fn sd_capsule(p: vec3f, a: vec3f, b: vec3f, r: f32) -> f32 {
  let pa = p - a;
  let ba = b - a;
  let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
  return length(pa - ba * h) - r;
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

/** Ellipsoid: the library bound outside; (k0 − 1)·r_min inside (the wolf author's fix, a true bound). */
fn sd_ell(p: vec3f, r: vec3f) -> f32 {
  let k0 = length(p / r);
  if (k0 < 1.0) { return (k0 - 1.0) * min(r.x, min(r.y, r.z)); }
  let k1 = length(p / (r * r));
  return k0 * (k0 - 1.0) / k1;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
  let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
  return mix(b, a, h) - k * h * (1.0 - h);
}

fn smax(a: f32, b: f32, k: f32) -> f32 { return -smin(-a, -b, k); }

fn rot_x(p: vec3f, a: f32) -> vec3f { let c = cos(a); let s = sin(a); return vec3f(p.x, c * p.y - s * p.z, s * p.y + c * p.z); }
fn rot_y(p: vec3f, a: f32) -> vec3f { let c = cos(a); let s = sin(a); return vec3f(c * p.x + s * p.z, p.y, -s * p.x + c * p.z); }
fn rot_z(p: vec3f, a: f32) -> vec3f { let c = cos(a); let s = sin(a); return vec3f(c * p.x - s * p.y, s * p.x + c * p.y, p.z); }
fn mirror_x(p: vec3f) -> vec3f { return vec3f(abs(p.x), p.y, p.z); }

/** A 2D shape of signed distance d2 extruded to half-thickness th. */
fn extrude(d2: f32, z: f32, th: f32) -> f32 {
  let w = vec2f(d2, abs(z) - th);
  return min(max(w.x, w.y), 0.0) + length(max(w, vec2f(0.0)));
}

fn safe_inv(x: f32) -> f32 { return 1.0 / select(x, select(-1e-9, 1e-9, x >= 0.0), abs(x) < 1e-9); }

/** Ray against a sphere: (t_near, t_far), or (INF, -INF) on a miss. */
fn ray_sphere(o: vec3f, d: vec3f, c: vec3f, r: f32) -> vec2f {
  let oc = o - c;
  let b = dot(oc, d);
  let h = b * b - (dot(oc, oc) - r * r);
  if (h < 0.0) { return vec2f(INF, -INF); }
  let s = sqrt(h);
  return vec2f(-b - s, -b + s);
}

fn ray_box(o: vec3f, d: vec3f, lo: vec3f, hi: vec3f) -> vec2f {
  let inv = vec3f(safe_inv(d.x), safe_inv(d.y), safe_inv(d.z));
  let a = (lo - o) * inv;
  let b = (hi - o) * inv;
  let t0 = max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z));
  let t1 = min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z));
  if (t1 < max(t0, 0.0)) { return vec2f(INF, -INF); }
  return vec2f(max(t0, 0.0), t1);
}

// ---- sky and light (golden hour) ---------------------------------------------------------------------

const ZENITH = vec3f(0.13, 0.25, 0.55);
const HOR_AWAY = vec3f(0.52, 0.55, 0.62);
const HOR_SUN = vec3f(1.05, 0.60, 0.30);

/** Sky radiance in direction d. `disc` includes the sun's disc (off for reflections: the GGX lobe
 *  carries the sun). */
fn sky_rad(d: vec3f, disc: bool) -> vec3f {
  let s = F.sun.xyz;
  let mu = dot(d, s);
  let y = max(d.y, 0.0);
  let az = 0.5 + 0.5 * dot(normalize(d.xz + vec2f(1e-5, 0.0)), normalize(s.xz));
  let hor = mix(HOR_AWAY, HOR_SUN, az * az * az);
  var c = mix(hor, ZENITH, pow(smoothstep(0.0, 0.85, y), 0.55));
  c += vec3f(1.6, 0.85, 0.38) * pow(max(mu, 0.0), 10.0) * 0.55;
  c += vec3f(3.0, 1.9, 1.0) * pow(max(mu, 0.0), 180.0);
  if (disc) { c += vec3f(60.0, 45.0, 30.0) * smoothstep(0.99996, 0.99999, mu); }
  if (d.y < 0.0) { c = mix(c, vec3f(0.20, 0.18, 0.15), smoothstep(0.0, -0.25, d.y)); }
  return c;
}

/** A distant treeline painted into the sky near the horizon (F.prm.w = its strength), so forest
 *  scenes don't sit on a bare plain. Hazed toward the horizon colour. */
fn treeline(d: vec3f, c: vec3f) -> vec3f {
  if (F.prm.w <= 0.0) { return c; }
  let az = atan2(d.z, d.x);
  let k = vec3f(cos(az), sin(az), 0.0);
  let h = 0.035 + 0.05 * (0.5 + 0.5 * noise3(k * 9.0)) + 0.03 * noise3(k * 45.0 + 3.0) + 0.012 * abs(noise3(k * 160.0 + 7.0));
  let m = 1.0 - smoothstep(h - 0.004, h + 0.004, d.y);
  let haze = mix(vec3f(0.10, 0.13, 0.11), c, 0.55 + 0.25 * smoothstep(0.0, h, d.y));
  return mix(c, haze, m * F.prm.w);
}

/** Irradiance-ish sky light reaching a surface with normal n (divided by π: multiply by albedo). */
fn sky_light(n: vec3f) -> vec3f {
  let up = 0.5 + 0.5 * n.y;
  let s = F.sun.xyz;
  let toward = max(dot(normalize(n.xz + vec2f(1e-5, 0.0)), normalize(s.xz)), 0.0) * (1.0 - abs(n.y));
  let skyc = mix(vec3f(0.42, 0.44, 0.48), vec3f(0.36, 0.48, 0.72), up);
  let ground = vec3f(0.16, 0.13, 0.09);
  return mix(ground, skyc, smoothstep(0.0, 1.0, up)) + vec3f(0.30, 0.17, 0.08) * toward;
}

/** Sky reflection blurred by roughness (no sun disc: the GGX term carries it). */
fn sky_env(r: vec3f, rough: f32) -> vec3f {
  let sharp = sky_rad(normalize(vec3f(r.x, max(r.y, -0.2), r.z)), false);
  return mix(sharp, sky_light(r) * 1.1, smoothstep(0.1, 0.7, rough));
}

fn tonemap(x: vec3f) -> vec3f {
  let c = x * F.sunc.w;
  return clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), vec3f(0.0), vec3f(1.0));
}

/** Split-sum environment BRDF, Karis' analytic fit (mobile, 2014): the share of a blurred
 *  environment a surface reflects, by roughness and n·v. */
fn env_brdf(f0: f32, rough: f32, nv: f32) -> f32 {
  let r = rough * vec4f(-1.0, -0.0275, -0.572, 0.022) + vec4f(1.0, 0.0425, 1.04, -0.04);
  let a004 = min(r.x * r.x, exp2(-9.28 * nv)) * r.x + r.y;
  let ab = vec2f(-1.04, 1.04) * a004 + r.zw;
  return f0 * ab.x + ab.y;
}

fn fresnel(f0: f32, c: f32) -> f32 { return f0 + (1.0 - f0) * pow(1.0 - clamp(c, 0.0, 1.0), 5.0); }

/** GGX specular × n·l (Smith height-correlated visibility). a2 = roughness⁴, already widened. */
fn ggx(n: vec3f, v: vec3f, l: vec3f, a2: f32, f0: f32) -> f32 {
  let h = normalize(v + l);
  let ndl = max(dot(n, l), 0.0);
  let ndv = max(dot(n, v), 1e-4);
  let ndh = max(dot(n, h), 0.0);
  let dd = ndh * ndh * (a2 - 1.0) + 1.0;
  let D = a2 / (PI * dd * dd);
  let vis = 0.5 / (ndl * sqrt(ndv * ndv * (1.0 - a2) + a2) + ndv * sqrt(ndl * ndl * (1.0 - a2) + a2) + 1e-6);
  return D * vis * fresnel(f0, dot(v, h)) * ndl;
}

/** Henyey–Greenstein phase function. */
fn hg(c: f32, g: f32) -> f32 {
  let d = 1.0 + g * g - 2.0 * g * c;
  return (1.0 - g * g) / (4.0 * PI * d * sqrt(d));
}
