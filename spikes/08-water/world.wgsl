// The world's layout as pure functions: noise, terrain, the lake's shape, the stream's course and
// level, and where trees may grow. No bindings, so placement (place.wgsl) and tracing share it.

const INF: f32 = 1e30;
const PI: f32 = 3.14159265;

// ---- noise -----------------------------------------------------------------------------------------

fn hash_u(x: u32) -> u32 {
  var h = x * 747796405u + 2891336453u;
  h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
  return (h >> 22u) ^ h;
}

fn hash2u(i: vec2i, s: u32) -> u32 {
  let u = bitcast<vec2u>(i);
  return hash_u(u.x + hash_u(u.y + hash_u(s)));
}

/** Uniform in [0, 1). */
fn h01(i: vec2i, s: u32) -> f32 { return f32(hash2u(i, s) >> 8u) * (1.0 / 16777216.0); }

fn lat2(i: vec2i) -> f32 { return f32(hash2u(i, 0u) >> 8u) * (2.0 / 16777216.0) - 1.0; }

/** 2D value noise with quintic fade, in [−1, 1]. */
fn noise2(x: vec2f) -> f32 {
  let fl = floor(x);
  let i = vec2i(fl);
  let f = x - fl;
  let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  let a = lat2(i);
  let b = lat2(i + vec2i(1, 0));
  let c = lat2(i + vec2i(0, 1));
  let d = lat2(i + vec2i(1, 1));
  return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

/** Value and gradient (x, y) of noise2. */
fn noise2_g(x: vec2f) -> vec3f {
  let fl = floor(x);
  let i = vec2i(fl);
  let f = x - fl;
  let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  let du = 30.0 * f * f * (f * (f - 2.0) + 1.0);
  let a = lat2(i);
  let b = lat2(i + vec2i(1, 0));
  let c = lat2(i + vec2i(0, 1));
  let d = lat2(i + vec2i(1, 1));
  let k = a - b - c + d;
  return vec3f(a + (b - a) * u.x + (c - a) * u.y + k * u.x * u.y,
               du * vec2f(b - a + k * u.y, c - a + k * u.x));
}

fn lat3(c: vec3i) -> f32 {
  let u = bitcast<vec3u>(c);
  return f32(hash_u(u.x + hash_u(u.y + hash_u(u.z))) >> 8u) * (2.0 / 16777216.0) - 1.0;
}

/** 3D value noise (spike 02's), in [−1, 1]. */
fn noise3(x: vec3f) -> f32 {
  let fl = floor(x);
  let i = vec3i(fl);
  let f = x - fl;
  let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  let a = lat3(i);
  let b = lat3(i + vec3i(1, 0, 0));
  let c = lat3(i + vec3i(0, 1, 0));
  let d = lat3(i + vec3i(1, 1, 0));
  let e = lat3(i + vec3i(0, 0, 1));
  let f1 = lat3(i + vec3i(1, 0, 1));
  let g = lat3(i + vec3i(0, 1, 1));
  let h = lat3(i + vec3i(1, 1, 1));
  return mix(mix(mix(a, b, u.x), mix(c, d, u.x), u.y), mix(mix(e, f1, u.x), mix(g, h, u.x), u.y), u.z);
}

/** Fades an octave of frequency `freq` (cycles per metre) out as the footprint `fp` (metres)
 *  approaches its Nyquist limit: the bandlimit (D-077). */
fn band(fp: f32, freq: f32) -> f32 { return 1.0 - smoothstep(0.25, 0.5, fp * freq); }

// ---- the lake ---------------------------------------------------------------------------------------

const LAKE_R0: f32 = 26.0;
const LAKE_RMIN: f32 = 21.2;            // 26 − 3 − 1.8: no shoreline point is nearer the centre

fn lake_R(xz: vec2f) -> f32 {
  let th = atan2(xz.y, xz.x);
  return LAKE_R0 + 3.0 * sin(2.0 * th + 0.5) + 1.8 * sin(3.0 * th + 2.1);
}

/** Distance from the lake's centre over the shoreline radius in that direction: 1 at the shore. */
fn lake_rn(xz: vec2f) -> f32 { return length(xz) / lake_R(xz); }

// ---- the stream -------------------------------------------------------------------------------------
// It runs north-east from its mouth on the lake's north-east shore. s: metres upstream from the
// mouth; l: metres across, from the meandering centreline.

const S_MOUTH = vec2f(15.0, -15.0);
const S_U = vec2f(0.70710678, -0.70710678);   // upstream
const S_V = vec2f(0.70710678, 0.70710678);    // across
const S_FALL: f32 = 8.0;                      // where it falls
const FALL_H: f32 = 2.0;
const S_SLOPE: f32 = 0.045;
const S_HALF: f32 = 2.4;                      // the water surface's half-width (the banks hide its edges)
const S_END: f32 = 46.0;                      // upstream of this the stream is under the hills

fn meander(s: f32) -> f32 { return 1.4 * (sin(0.13 * s + 0.4) - sin(0.4)); }
fn meander_d(s: f32) -> f32 { return 0.182 * cos(0.13 * s + 0.4); }

/** (s, l) for a point. l has slope ≤ √(1 + 0.18²) in xz. */
fn stream_sl(xz: vec2f) -> vec2f {
  let q = xz - S_MOUTH;
  let s = dot(q, S_U);
  return vec2f(s, dot(q, S_V) - meander(s));
}

fn sstep_d(a: f32, b: f32, x: f32) -> f32 {
  let t = clamp((x - a) / (b - a), 0.0, 1.0);
  return 6.0 * t * (1.0 - t) / (b - a);
}

/** The stream's water level at s. The fall is a 0.6 m chute. */
fn stream_level(s: f32) -> f32 {
  return S_SLOPE * max(s, 0.0) + FALL_H * smoothstep(S_FALL - 0.3, S_FALL + 0.3, s);
}

fn stream_level_d(s: f32) -> f32 {
  return select(0.0, S_SLOPE, s > 0.0) + FALL_H * sstep_d(S_FALL - 0.3, S_FALL + 0.3, s);
}

/** The water surface's half-width: narrower where it falls, between the boulders. */
fn stream_half(s: f32) -> f32 {
  return S_HALF - 0.9 * (smoothstep(S_FALL - 0.9, S_FALL - 0.35, s) - smoothstep(S_FALL + 0.35, S_FALL + 0.9, s));
}

/** The bed's reference level: its step lags the water's so the chute has a pool under it. */
fn bed_level(s: f32) -> f32 {
  return S_SLOPE * max(s, 0.0) + FALL_H * smoothstep(S_FALL - 0.2, S_FALL + 2.2, s);
}

// ---- terrain -----------------------------------------------------------------------------------------

/** Ground height. Slope bounds: ≤ 0.72 away from the stream and the escarpment (the steep boxes
 *  in scene.wgsl), ≤ 1.6 inside them. Sampled maxima were 0.67 and 1.45; place.wgsl's probe
 *  re-checks both on every run. */
fn terrain(xz: vec2f) -> f32 {
  let x = xz.x;
  let z = xz.y;
  let r = length(xz);
  let rn = r / lake_R(xz);
  // Basin with a shallow shelf: depth falls to zero smoothly at the shore.
  var h = mix(-3.0, 0.0, smoothstep(0.38, 1.0, rn));
  // Land rising from the shore.
  let land = max(rn - 1.0, 0.0);
  h += 0.11 * LAKE_R0 * land * land / (land + 0.08);
  // Hills, fading in away from the shore.
  let hw = smoothstep(1.0, 1.9, rn);
  h += hw * (2.2 * sin(0.043 * x + 1.0) * cos(0.047 * z + 0.3) + 1.0 * sin(0.083 * x - 0.7 + 0.6 * cos(0.061 * z)));
  // The rim of the world: hills that close the horizon.
  let rim = max(r - 80.0, 0.0);
  h += 0.2 * rim * rim / (rim + 6.0);
  // Ground roughness.
  h += 0.10 * noise2(xz * 0.45 + 3.0) * smoothstep(0.85, 1.05, rn);
  h += 0.05 * noise2(xz * 0.9);
  // The escarpment the stream falls over, across the stream's whole course.
  let sl = stream_sl(xz);
  let esc = FALL_H * smoothstep(S_FALL - 2.5, S_FALL + 1.5, sl.x) * (1.0 - smoothstep(14.0, 28.0, abs(sl.y)));
  h += esc;
  // The stream's valley: a channel under the water, banks above it, blended into the land.
  // It fades out upstream: the stream comes out of the hills.
  let near = (1.0 - smoothstep(5.0, 11.0, abs(sl.y))) * smoothstep(-6.0, -1.0, sl.x) * (1.0 - smoothstep(30.0, 45.0, sl.x));
  if (near > 0.0) {
    let al = abs(sl.y);
    let valley = bed_level(sl.x) - 0.5 + 0.5 * smoothstep(0.0, 2.3, al) + 0.6 * smoothstep(1.7, 3.2, al)
               + 0.22 * max(al - 3.2, 0.0) + 0.05 * noise2(xz * 1.3);
    h = mix(h, valley, near);
  }
  return h;
}

fn terrain_n(xz: vec2f) -> vec3f {
  let e = 0.02;
  let hx = terrain(xz + vec2f(e, 0.0)) - terrain(xz - vec2f(e, 0.0));
  let hz = terrain(xz + vec2f(0.0, e)) - terrain(xz - vec2f(0.0, e));
  return normalize(vec3f(-hx, 2.0 * e, -hz));
}

// ---- trees ------------------------------------------------------------------------------------------
// One tree per 4.5 m cell, jittered, confined to its cell: jitter 0.45 + canopy 1.55 + leaf
// displacement 0.2 ≤ 2.25. So a ray walking cells evaluates one tree at a time.

const CELL: f32 = 4.5;
const GRID_N: u32 = 64u;
const GRID_ORIGIN: f32 = -144.0;
const NCELLS: u32 = 4096u;
const JITTER: f32 = 0.45;
const TREE_FREE_R: f32 = 19.0;           // LAKE_RMIN − 2.2: no tree reaches inside this disc
const TOWER_XZ = vec2f(-12.0, -33.0);
const SHORE_CAM_XZ = vec2f(6.0, 27.0);

fn tree_density(p: vec2f) -> f32 {
  let rn = lake_rn(p);
  let shore = smoothstep(1.04, 1.25, rn + 0.05 * noise2(p * 0.17));
  let world = 1.0 - smoothstep(128.0, 138.0, length(p));
  let sl = stream_sl(p);
  let stream = 1.0 - (1.0 - smoothstep(3.6, 5.5, abs(sl.y))) * smoothstep(-8.0, -4.0, sl.x) * (1.0 - smoothstep(40.0, 50.0, sl.x));
  let tower = smoothstep(8.5, 11.0, distance(p, TOWER_XZ));
  let clear = smoothstep(4.0, 8.0, distance(p, SHORE_CAM_XZ));
  let clump = 0.6 + 0.4 * smoothstep(-0.4, 0.3, noise2(p * 0.045 + 7.0));
  return 0.92 * shore * world * stream * tower * clear * clump;
}

/** Birches like the shore; spruce like the hills. */
fn birch_share(p: vec2f) -> f32 {
  return mix(0.45, 0.12, smoothstep(1.2, 2.2, lake_rn(p))) + 0.15 * noise2(p * 0.06 + 2.0);
}
