// Shared by every pass: the frame uniform, statistics, hashing and noise, the sky, octahedral maps,
// tonemapping. Prepended to scenes.wgsl and light.wgsl / ref.wgsl.

struct Frame {
  eye: vec4f,     // xyz camera; w: a pixel's angular size (2·tan(fovy/2) / height)
  cf: vec4f,      // camera forward
  cr: vec4f,      // camera right × tan(half fov x)
  cu: vec4f,      // camera up × tan(half fov y)
  sun: vec4f,     // xyz toward the sun; w: tan of the sun disc's angular radius
  sun_e: vec4f,   // rgb: sun irradiance at normal incidence; w: exposure
  sky_z: vec4f,   // rgb: zenith radiance; w: sun aureole strength
  sky_h: vec4f,   // rgb: horizon radiance; w: below-horizon factor
  fog: vec4f,     // rgb: fog radiance; w: density (1/m)
  dims: vec4u,    // render width, height; sky-pass width, height
  fr: vec4u,      // frame index, temporal history length H, rays per probe R, sky cones K
  lp: vec4f,      // sky cone length L, sun ray max distance, probe ray max distance, unused
  flags: vec4u,   // x: collect stats, y: view (0 all, 1 sun, 2 sky, 3 bounce), z: reset history, w: reference half (0 both, 1 A, 2 B)
  rot0: vec4f,    // this frame's random rotation for probe rays (rows)
  rot1: vec4f,
  rot2: vec4f,
  pg_o: vec4f,    // probe grid origin; w: probe update blend α
  pg_s: vec4f,    // probe spacing xyz; w: distance normalization for the depth moments
  pg_n: vec4u,    // probe counts xyz; w: total
  pe: vec4f,      // previous frame's camera (eye, forward, right, up), for reprojection
  pcf: vec4f,
  pcr: vec4f,
  pcu: vec4f,
  vol_o: vec4f,   // cooked distance clipmap, fine level: origin; w: voxel size
  vol_n: vec4u,   // fine level: dimensions; w: unused
  vol1_o: vec4f,  // coarse level: origin; w: voxel size
  vol1_n: vec4u,  // coarse level: dimensions; w: unused
  hm_o: vec4f,    // cooked terrain height map: xy origin (world x, z), z texel size, w unused
  hm_n: vec4u,    // height map dimensions
  rf: vec4u,      // reference: x sample-pair index, y seed, z pairs accumulated, w unused
  pa: vec4f,      // x: probe normal bias (× min spacing), y: probe view bias, z: sun rays' analytic near zone (m), w: sky cones' (m)
  rs: vec4u,      // x: sun visibility resolution divisor (1 or 2), y: probe gather divisor, z, w: unused
}

@group(0) @binding(0) var<uniform> F: Frame;
@group(0) @binding(1) var<storage, read_write> stats: array<atomic<u32>, 64>;

const PI: f32 = 3.14159265;
const INF: f32 = 1e30;

// Stats slots.
const S_PRIM_PX: u32 = 0u;
const S_PRIM_STEPS: u32 = 1u;
const S_PRIM_CAPS: u32 = 2u;
const S_PRIM_HITS: u32 = 3u;
const S_SUN_RAYS: u32 = 4u;
const S_SUN_STEPS: u32 = 5u;
const S_SUN_CAPS: u32 = 6u;
const S_SKY_CONES: u32 = 7u;
const S_SKY_STEPS: u32 = 8u;
const S_SKY_CAPS: u32 = 9u;
const S_PR_RAYS: u32 = 10u;
const S_PR_STEPS: u32 = 11u;
const S_PR_CAPS: u32 = 12u;
const S_PR_HITS: u32 = 13u;
const S_PS_RAYS: u32 = 14u;
const S_PS_STEPS: u32 = 15u;
const S_PS_CAPS: u32 = 16u;
const S_REF_RAYS: u32 = 17u;
const S_REF_STEPS: u32 = 18u;
const S_REF_CAPS: u32 = 19u;
const S_PROBE_ACTIVE: u32 = 20u;
const S_PROBE_SOLID: u32 = 21u;
const S_PROBE_FAR: u32 = 22u;
const S_PR_BACK: u32 = 23u;
const S_REF_SHADOW_RAYS: u32 = 24u;

fn stat(slot: u32, v: u32) {
  if (F.flags.x != 0u && v != 0u) { atomicAdd(&stats[slot], v); }
}

// ---- hashing and noise ---------------------------------------------------------------------------

fn ihash(n: u32) -> u32 {
  var x = n;
  x ^= x >> 16u;
  x *= 0x7feb352du;
  x ^= x >> 15u;
  x *= 0x846ca68bu;
  x ^= x >> 16u;
  return x;
}

/** Cheap integer hashes for noise and placement (a multiply-xorshift; good enough for value noise). */
fn hash2i(c: vec2i) -> u32 {
  var h = (bitcast<u32>(c.x) * 0x8da6b343u) ^ (bitcast<u32>(c.y) * 0xd8163841u);
  h = (h ^ (h >> 15u)) * 0x2c1b3c6du;
  return h ^ (h >> 12u);
}
fn hash3i(c: vec3i) -> u32 {
  var h = (bitcast<u32>(c.x) * 0x8da6b343u) ^ (bitcast<u32>(c.y) * 0xd8163841u) ^ (bitcast<u32>(c.z) * 0xcb1ab31fu);
  h = (h ^ (h >> 15u)) * 0x2c1b3c6du;
  return h ^ (h >> 12u);
}
fn u01(h: u32) -> f32 { return f32(h >> 8u) * (1.0 / 16777216.0); }
fn s11(h: u32) -> f32 { return f32(h >> 8u) * (2.0 / 16777216.0) - 1.0; }

/** 2D value noise in [-1, 1] with its gradient: (value, ∂/∂x, ∂/∂y). Cubic fade, so C1. */
fn vnoise2d(p: vec2f) -> vec3f {
  let i = vec2i(floor(p));
  let f = fract(p);
  let u = f * f * (3.0 - 2.0 * f);
  let du = 6.0 * f * (1.0 - f);
  let a = s11(hash2i(i));
  let b = s11(hash2i(i + vec2i(1, 0)));
  let c = s11(hash2i(i + vec2i(0, 1)));
  let d = s11(hash2i(i + vec2i(1, 1)));
  let k1 = b - a;
  let k2 = c - a;
  let k4 = a - b - c + d;
  return vec3f(a + k1 * u.x + k2 * u.y + k4 * u.x * u.y, du.x * (k1 + k4 * u.y), du.y * (k2 + k4 * u.x));
}

fn vnoise2(p: vec2f) -> f32 {
  let i = vec2i(floor(p));
  let f = fract(p);
  let u = f * f * (3.0 - 2.0 * f);
  return mix(mix(s11(hash2i(i)), s11(hash2i(i + vec2i(1, 0))), u.x),
             mix(s11(hash2i(i + vec2i(0, 1))), s11(hash2i(i + vec2i(1, 1))), u.x), u.y);
}

/** 3D value noise in [-1, 1]. */
fn vnoise3(p: vec3f) -> f32 {
  let i = vec3i(floor(p));
  let f = fract(p);
  let u = f * f * (3.0 - 2.0 * f);
  let a = mix(s11(hash3i(i)), s11(hash3i(i + vec3i(1, 0, 0))), u.x);
  let b = mix(s11(hash3i(i + vec3i(0, 1, 0))), s11(hash3i(i + vec3i(1, 1, 0))), u.x);
  let c = mix(s11(hash3i(i + vec3i(0, 0, 1))), s11(hash3i(i + vec3i(1, 0, 1))), u.x);
  let d = mix(s11(hash3i(i + vec3i(0, 1, 1))), s11(hash3i(i + vec3i(1, 1, 1))), u.x);
  return mix(mix(a, b, u.y), mix(c, d, u.y), u.z);
}

// ---- camera ---------------------------------------------------------------------------------------

/** World ray direction through pixel `px` (pixel centre at +0.5) of a w×h image. */
fn ray_dir(px: vec2f, w: f32, h: f32) -> vec3f {
  let u = px.x / w * 2.0 - 1.0;
  let v = 1.0 - px.y / h * 2.0;
  return normalize(F.cf.xyz + F.cr.xyz * u + F.cu.xyz * v);
}

// ---- sky ------------------------------------------------------------------------------------------

/** Sky radiance in direction d, excluding the sun disc (the sun is handled as its own light). */
fn sky_radiance(d: vec3f) -> vec3f {
  let y = d.y;
  var c = mix(F.sky_h.rgb, F.sky_z.rgb, pow(clamp(y, 0.0, 1.0), 0.5));
  if (y < 0.0) { c = F.sky_h.rgb * mix(1.0, F.sky_h.w, clamp(-y * 6.0, 0.0, 1.0)); }
  let mu = max(dot(d, F.sun.xyz), 0.0);
  c += F.sun_e.rgb * F.sky_z.w * (0.012 * pow(mu, 6.0) + 0.04 * pow(mu, 48.0));
  return c;
}

// ---- octahedral maps --------------------------------------------------------------------------------

fn sign_nz(v: vec2f) -> vec2f { return select(vec2f(-1.0), vec2f(1.0), v >= vec2f(0.0)); }

fn oct_encode(v: vec3f) -> vec2f {
  let l1 = abs(v.x) + abs(v.y) + abs(v.z);
  var r = v.xz / l1;
  if (v.y < 0.0) { r = (1.0 - abs(r.yx)) * sign_nz(r); }
  return r;
}

fn oct_decode(e: vec2f) -> vec3f {
  var v = vec3f(e.x, 1.0 - abs(e.x) - abs(e.y), e.y);
  if (v.y < 0.0) {
    let xz = (1.0 - abs(v.zx)) * sign_nz(v.xz);
    v = vec3f(xz.x, v.y, xz.y);
  }
  return normalize(v);
}

/** For a texel of an n×n interior octahedral tile with a one-texel border (size n+2), the interior
 *  texel (1..n) whose value it holds. Border texels mirror across the octahedron's edges (DDGI). */
fn oct_border_src(t: vec2u, n: u32) -> vec2u {
  let e = n + 1u;
  var s = t;
  let bx = t.x == 0u || t.x == e;
  let by = t.y == 0u || t.y == e;
  if (bx && by) {
    s = vec2u(select(1u, n, t.x == 0u), select(1u, n, t.y == 0u));
  } else if (by) {
    s = vec2u(e - t.x, select(n, 1u, t.y == 0u));
  } else if (bx) {
    s = vec2u(select(n, 1u, t.x == 0u), e - t.y);
  }
  return s;
}

/** The direction an interior texel (1..n) of an n×n octahedral tile stands for. */
fn oct_texel_dir(s: vec2u, n: u32) -> vec3f {
  let uv = (vec2f(s) - 0.5) / f32(n);
  return oct_decode(uv * 2.0 - 1.0);
}

// ---- sampling helpers ----------------------------------------------------------------------------------

/** An orthonormal basis around n (Duff et al. 2017). */
fn basis(n: vec3f) -> mat3x3f {
  let s = select(-1.0, 1.0, n.z >= 0.0);
  let a = -1.0 / (s + n.z);
  let b = n.x * n.y * a;
  return mat3x3f(vec3f(1.0 + s * n.x * n.x * a, s * b, -s * n.x), vec3f(b, s + n.y * n.y * a, -n.y), n);
}

/** Spherical Fibonacci direction i of n. */
fn sf_dir(i: u32, n: u32) -> vec3f {
  let phi = 2.0 * PI * fract(f32(i) * 0.6180339887);
  let z = 1.0 - (2.0 * f32(i) + 1.0) / f32(n);
  let r = sqrt(max(1.0 - z * z, 0.0));
  return vec3f(r * cos(phi), z, r * sin(phi));
}

fn probe_ray_dir(i: u32, n: u32) -> vec3f {
  let d = sf_dir(i, n);
  return vec3f(dot(F.rot0.xyz, d), dot(F.rot1.xyz, d), dot(F.rot2.xyz, d));
}

/** Interleaved gradient noise (Jimenez 2014), animated per frame. */
fn ign(px: vec2f, frame: u32) -> f32 {
  let p = px + 5.588238 * f32(frame % 64u);
  return fract(52.9829189 * fract(dot(p, vec2f(0.06711056, 0.00583715))));
}

// ---- display ------------------------------------------------------------------------------------------

fn tonemap(x: vec3f) -> vec3f {
  let c = x * 0.8;
  return clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), vec3f(0.0), vec3f(1.0));
}
