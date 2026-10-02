// Bindings shared by the tracing passes, then the world's objects (trees, rocks, tower), the sky,
// the tracer and shading. Appended to world.wgsl.

struct Frame {
  eye: vec4f,       // xyz: camera; w: a pixel's angular size at the internal resolution (radians)
  cf: vec4f,        // camera forward
  cr: vec4f,        // camera right × tan(half fov x)
  cu: vec4f,        // camera up × tan(half fov y)
  sun: vec4f,       // xyz: toward the sun; w: its angular radius
  sun_col: vec4f,   // sun irradiance colour; w: exposure
  zen: vec4f,       // sky at the zenith; w: cloud cover
  hor: vec4f,       // sky at the horizon away from the sun; w: fog density (per metre)
  hor_sun: vec4f,   // sky at the horizon toward the sun; w: shadow ray length (m)
  dims: vec4u,      // internal width, height; reflection divisor; refraction divisor
  misc: vec4f,      // time (s); reflection length (m); jitter x, y (pixels)
  wind: vec4f,      // wind direction xz; ripple strength; unused
  wv: vec4f,        // the active wave table's amplitude sum and slope sum (bounds); unused ×2
  rocks: vec4u,     // rock count; how many lead the list as the stream group (inside rlo..rhi)
  rlo: vec4f,       // the rocks' bounding box
  rhi: vec4f,
  tile: vec4u,      // the pixels this submission covers: x0, y0, x1, y1 (full resolution)
}

struct Wave { a: vec4f, b: vec4f }   // a: direction xz, wavenumber, amplitude; b: phase, ω

@group(0) @binding(0) var<uniform> F: Frame;
@group(0) @binding(1) var<storage, read_write> stats: array<atomic<u32>, 48>;
@group(0) @binding(2) var<storage, read> objs: array<vec4f>;
@group(0) @binding(3) var<uniform> WV: array<Wave, 42>;
@group(0) @binding(4) var hmap: texture_2d<f32>;      // the terrain, cooked once (place.wgsl's bake)
@group(0) @binding(5) var hsamp: sampler;
@group(0) @binding(6) var ntex3: texture_3d<f32>;     // noise3, cooked: 96³ samples of 16 lattice units
@group(0) @binding(7) var ntex2: texture_2d<f32>;     // noise2 and its gradient: 512² samples of 64 units
@group(0) @binding(8) var nsamp: sampler;             // linear, repeat
@group(0) @binding(9) var<uniform> RS: array<vec4f, 32>;  // rock bound spheres: centre, radius

// Value noise from cooked tables (place.wgsl's bake_noise): the same lattice noise as world.wgsl's,
// tiled and sampled 6 (3D) or 8 (2D) times per lattice unit, trilinear/bilinear. One fetch instead
// of 8 (3D) or 4 (2D) hashed lattice points. The analytic versions are kept for cooking.
fn tnoise3(p: vec3f) -> f32 { return textureSampleLevel(ntex3, nsamp, p * (1.0 / 16.0), 0.0).x; }
fn tnoise2(p: vec2f) -> f32 { return textureSampleLevel(ntex2, nsamp, p * (1.0 / 64.0), 0.0).x; }
fn tnoise2_g(p: vec2f) -> vec3f { return textureSampleLevel(ntex2, nsamp, p * (1.0 / 64.0), 0.0).xyz; }

// Variants are pipeline-overridable constants, so each compiles to its own specialized kernel.
override REF: bool = false;          // brute-force reference: half steps, 1/20 tolerance, huge caps
override STATS: bool = false;        // count steps, caps and hits with atomics
override VIEW: u32 = 0u;             // 0 shaded; 1 reflection-step heat; 2 refraction-step heat; 3 scene-step heat
override WAVES: u32 = 1u;            // wave detail: 0 low, 1 mid, 2 high
override PROXY: bool = false;        // trace a coarse proxy of the world (reflections only)
override WATER: bool = true;         // false: a dry lake (the marginal-cost baseline)
override NEWTON: u32 = 2u;           // Newton steps refining the lake's mean-plane hit
override PROFILE: u32 = 0u;          // cost breakdown only (reflect_pass): 1 hits unshaded; 2 no trace; 3 no rocks or tower; 4 terrain only

override STEP: f32 = select(1.0, 0.5, REF);
override EPS: f32 = select(0.25, 0.0125, REF);      // hit tolerance, in pixel footprints (spike 02: ¼ passes, ½ fails)
// Reference caps are in the low thousands, and references run in 96² tiles (one submission each).
override MAX_STEPS: u32 = select(160u, 3000u, REF); // object steps per ray
override T_STEPS: u32 = select(128u, 3000u, REF);   // terrain steps per ray
override SH_STEPS: u32 = select(96u, 2000u, REF);   // shadow steps per ray
override W_STEPS: u32 = select(48u, 3000u, REF);    // water-surface steps per ray

// Object buffer layout (vec4s): 2 per tree cell, 2 per rock, the tower, probes.
const ROCK0: u32 = 8192u;
const MAX_ROCKS: u32 = 32u;
const TOWER_SLOT: u32 = 8256u;
const PROBE0: u32 = 8257u;

const K_SKY: u32 = 0u;
const K_TERRAIN: u32 = 1u;
const K_TREE: u32 = 2u;
const K_TOWER: u32 = 3u;
const K_ROCK: u32 = 4u;
const K_FAR: u32 = 5u;

const M_BOUND: u32 = 0u;   // displacement replaced by its bound: a lower bound, safe to step on
const M_FULL: u32 = 1u;
const M_PROXY: u32 = 2u;   // the coarse proxy: no displacement, simpler shapes
const M_BASE: u32 = 3u;    // full shapes without leaf displacement: what casts sun shadows

const TREE_YMAX: f32 = 52.0;
const SOFT: f32 = 10.0;    // soft-shadow sharpness (penumbra ∝ distance / SOFT)

struct Hit { t: f32, kind: u32, id: u32 }
struct Cone { fp0: f32, spread: f32 }      // a ray's footprint: fp0 + spread·t metres

fn fp_at(c: Cone, t: f32) -> f32 { return c.fp0 + c.spread * t; }

struct Counters { steps: u32, limit: u32, caps: u32, cells: u32, tsteps: u32, tcaps: u32, shsteps: u32, wsteps: u32, wcaps: u32 }
var<private> C: Counters;

// Stats slots.
const S_WATER_PX: u32 = 0u;
const S_WS_STEPS: u32 = 1u;
const S_WS_CAPS: u32 = 2u;
const S_SCENE_STEPS: u32 = 3u;
const S_SCENE_CAPS: u32 = 4u;
const S_SCENE_TSTEPS: u32 = 5u;
const S_SCENE_CELLS: u32 = 6u;
const S_SHADOW_STEPS: u32 = 7u;
const S_REFL_RAYS: u32 = 8u;
const S_REFL_STEPS: u32 = 9u;
const S_REFL_CAPS: u32 = 10u;
const S_REFL_TSTEPS: u32 = 11u;
const S_REFL_CELLS: u32 = 12u;
const S_REFL_SKY: u32 = 13u;       // + kind: 13 sky, 14 terrain, 15 tree, 16 tower, 17 rock, 18 far
const S_REFR_RAYS: u32 = 19u;
const S_REFR_STEPS: u32 = 20u;
const S_REFR_CAPS: u32 = 21u;
const S_REFR_TSTEPS: u32 = 22u;
const S_REFR_HIT: u32 = 23u;
const S_GLINT_RAYS: u32 = 24u;
const S_GLINT_STEPS: u32 = 25u;
const S_STREAM_PX: u32 = 26u;
const S_REFL_WSTEPS: u32 = 27u;
const S_REFR_WSTEPS: u32 = 28u;
const S_SCENE_TCAPS: u32 = 29u;
const S_REFL_TCAPS: u32 = 30u;
const S_REFR_TCAPS: u32 = 31u;
const S_SKY_PX: u32 = 32u;
const S_REFL_FALLBACK: u32 = 33u;  // reduced-resolution texels whose centre ray missed the water
const S_GB_STEPS: u32 = 34u;       // Newton steps in water_gbuf
const S_SH_CAPS: u32 = 35u;        // shadow rays that ran out of steps (counted as lit)

// ---- small geometry ----------------------------------------------------------------------------------

fn safe_inv(x: f32) -> f32 { return 1.0 / select(x, select(-1e-9, 1e-9, x >= 0.0), abs(x) < 1e-9); }

fn box_iv(o: vec3f, d: vec3f, lo: vec3f, hi: vec3f) -> vec2f {
  let inv = vec3f(safe_inv(d.x), safe_inv(d.y), safe_inv(d.z));
  let a = (lo - o) * inv;
  let b = (hi - o) * inv;
  let t0 = max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z));
  let t1 = min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z));
  return vec2f(t0, t1);
}

fn sphere_iv(o: vec3f, d: vec3f, c: vec3f, r: f32) -> vec2f {
  let oc = o - c;
  let b = dot(oc, d);
  let h = b * b - dot(oc, oc) + r * r;
  if (h < 0.0) { return vec2f(INF, -INF); }
  let s = sqrt(h);
  return vec2f(-b - s, -b + s);
}

/** A vertical cylinder of radius r around c.xz, from c.y + y0 to c.y + y1. */
fn cyl_iv(o: vec3f, d: vec3f, c: vec3f, r: f32, y0: f32, y1: f32) -> vec2f {
  let oc = o.xz - c.xz;
  let a = dot(d.xz, d.xz);
  var t0 = -INF;
  var t1 = INF;
  if (a > 1e-12) {
    let b = dot(oc, d.xz);
    let h = b * b - a * (dot(oc, oc) - r * r);
    if (h < 0.0) { return vec2f(INF, -INF); }
    let s = sqrt(h);
    t0 = (-b - s) / a;
    t1 = (-b + s) / a;
  } else if (dot(oc, oc) > r * r) { return vec2f(INF, -INF); }
  let iy = safe_inv(d.y);
  let ya = (c.y + y0 - o.y) * iy;
  let yb = (c.y + y1 - o.y) * iy;
  return vec2f(max(t0, min(ya, yb)), min(t1, max(ya, yb)));
}

/** spike 01's ellipsoid bound (k0(k0−1)/k1). Its gradient is unbounded deep inside (D-092), but a
 *  march only evaluates it outside. */
fn ellipsoid_d(p: vec3f, r: vec3f) -> f32 {
  let k0 = length(p / r);
  let k1 = max(length(p / (r * r)), 1e-9);
  return k0 * (k0 - 1.0) / k1;
}

// ---- sky -----------------------------------------------------------------------------------------------

fn horizon_col(d: vec3f) -> vec3f {
  let a = dot(normalize(d.xz + vec2f(1e-6, 0.0)), normalize(F.sun.xz + vec2f(1e-6, 0.0)));
  return mix(F.hor.rgb, F.hor_sun.rgb, pow(0.5 + 0.5 * a, 3.0));
}

/** Diffuse light from the sky on a surface whose normal has this y. */
fn sky_amb(ny: f32) -> vec3f {
  let up = 0.55 * F.zen.rgb + 0.45 * 0.5 * (F.hor.rgb + F.hor_sun.rgb);
  let down = vec3f(0.035, 0.04, 0.025) + 0.1 * F.sun_col.rgb * max(F.sun.y, 0.0);
  return mix(down, up, 0.5 + 0.5 * ny);
}

fn clouds(base: vec3f, d: vec3f, theta: f32) -> vec3f {
  let dy = max(d.y, 0.02);
  let t = (1100.0 - F.eye.y) / dy;
  let uv = (F.eye.xz + d.xz * t) * (1.0 / 900.0) + F.wind.xy * F.misc.x * 0.0004;
  let fp = t * theta / dy / 900.0;
  var n = 0.0;
  var a = 0.5;
  var f = 1.0;
  for (var o = 0u; o < 5u; o++) {
    let w = band(fp, f);
    if (w <= 0.0) { break; }
    n += a * w * tnoise2(uv * f + vec2f(f32(o) * 7.1, f32(o) * 3.3));
    a *= 0.5;
    f *= 2.03;
  }
  let cover = F.zen.w;
  let dens = smoothstep(0.62 - cover, 1.0 - cover, n + 0.5) * smoothstep(0.02, 0.14, d.y);
  let mu = max(dot(d, F.sun.xyz), 0.0);
  let lit = 0.5 * F.hor_sun.rgb + 0.5 * F.zen.rgb + F.sun_col.rgb * (0.03 + 0.18 * pow(mu, 6.0));
  let col = mix(lit, lit * 0.55 + 0.15 * F.hor.rgb, smoothstep(0.35, 0.95, n + 0.5));
  return mix(base, col, dens * 0.9);
}

/** Sky radiance. `sun_disk` is false for reflection rays: the glint term draws the sun there. */
fn sky(d: vec3f, sun_disk: bool, theta: f32) -> vec3f {
  let mu = dot(d, F.sun.xyz);
  let y = max(d.y, 0.0);
  let hor = horizon_col(d);
  var c = mix(hor, F.zen.rgb, pow(y, 0.5));
  let g = max(mu, 0.0);
  c += F.sun_col.rgb * (0.025 * pow(g, 6.0) + 0.06 * pow(g, 90.0)) * (1.0 - 0.7 * y);
  if (d.y < 0.0) { c = mix(c, hor * 0.7, smoothstep(0.0, 0.15, -d.y)); }
  if (F.zen.w > 0.0 && d.y > 0.0) { c = clouds(c, d, theta); }
  if (sun_disk) {
    let e = max(theta, 2e-4);
    c += F.sun_col.rgb * 40.0 * (1.0 - smoothstep(F.sun.w - e, F.sun.w + e, acos(clamp(mu, -1.0, 1.0))));
  }
  return c;
}

/** Aerial perspective: scattered light toward the horizon colour, warmer toward the sun. */
fn fog(c: vec3f, t: f32, d: vec3f) -> vec3f {
  let f = 1.0 - exp(-t * F.hor.w);
  let mu = max(dot(d, F.sun.xyz), 0.0);
  let fc = horizon_col(d) + F.sun_col.rgb * 0.04 * pow(mu, 8.0);
  return mix(c, fc, f);
}

fn tonemap(x: vec3f) -> vec3f {
  let c = x * F.sun_col.w;
  return clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), vec3f(0.0), vec3f(1.0));
}

// ---- trees -----------------------------------------------------------------------------------------------
// Spruce: a trunk and a tiered cone. Birch: a trunk and two leafy ellipsoids. Displacement is
// value noise, faded by the footprint (the bandlimit). NOISE_SLOPE bounds |∇noise3| per unit of
// frequency: place.wgsl's probe measured a max of 3.69 over 1M points (spike 02 assumed 3.0).

const NOISE_SLOPE: f32 = 3.7;
const SP_DISP: f32 = 0.28;
const BI_DISP: f32 = 0.37;
const SP_L: f32 = 1.0 + (0.2 * 1.5 + 0.08 * 3.2) * NOISE_SLOPE + 0.14;   // + the taper's 0.5 × 0.28
const BI_L: f32 = 1.0 + (0.25 * 1.3 + 0.12 * 2.9) * NOISE_SLOPE;

/** (distance, part: 0 foliage, 1 trunk) in the tree's frame. */
fn spruce_d(q: vec3f, h: f32, r: f32, tr: f32, seed: f32, fp: f32, mode: u32) -> vec2f {
  let y0 = 0.16 * h;
  let rxz = length(q.xz);
  let trunk = max(rxz - tr, max(-q.y - 0.3, q.y - 0.6 * h));
  let u = clamp((q.y - y0) / (h - y0), 0.0, 1.0);
  var rad = r * (1.0 - u);
  var k = 0.98;
  if (mode != M_PROXY) {
    // Tiers: |d rad/dy| ≤ r (1/(h − y0) + 0.24·0.5·2π/1.3) ≤ 1.07 for r ≤ 1.5, h ≥ 9, so k = 0.68.
    rad *= 0.76 + 0.24 * (0.5 + 0.5 * cos(4.8332 * (q.y - y0) + seed * 6.0));
    k = 0.68;
  } else {
    rad *= 0.88;
  }
  var crown = max((rxz - rad) * k, max(y0 - q.y, q.y - h));
  if (mode == M_BOUND) {
    crown -= SP_DISP;
  } else if (mode == M_FULL) {
    let p = q + vec3f(seed * 17.0, 0.0, seed * 31.0);
    let taper = clamp((h - q.y) * 0.5, 0.0, 1.0);   // no loose blobs above the tip
    crown -= taper * (0.2 * band(fp, 1.5) * tnoise3(p * 1.5) + 0.08 * band(fp, 3.2) * tnoise3(p * 3.2 + 5.0));
  }
  return select(vec2f(crown, 0.0), vec2f(trunk, 1.0), trunk < crown);
}

fn birch_d(q: vec3f, h: f32, r: f32, tr: f32, seed: f32, fp: f32, mode: u32) -> vec2f {
  let rxz = length(q.xz);
  let trunk = max(rxz - tr, max(-q.y - 0.3, q.y - 0.8 * h));
  var crown = ellipsoid_d(q - vec3f(0.0, 0.6 * h, 0.0), vec3f(r, 0.3 * h, r));
  if (mode != M_PROXY) {
    let ang = seed * 6.2831853;
    crown = min(crown, ellipsoid_d(q - vec3f(0.3 * r * cos(ang), 0.82 * h, 0.3 * r * sin(ang)), vec3f(0.65 * r, 0.2 * h, 0.65 * r)));
  }
  if (mode == M_BOUND) {
    crown -= BI_DISP;
  } else if (mode == M_FULL) {
    let p = q + vec3f(seed * 23.0, 0.0, seed * 11.0);
    crown -= 0.25 * band(fp, 1.3) * tnoise3(p * 1.3) + 0.12 * band(fp, 2.9) * tnoise3(p * 2.9 + 3.0);
  }
  return select(vec2f(crown, 0.0), vec2f(trunk, 1.0), trunk < crown);
}

fn tree_eval(a: vec4f, b: vec4f, p: vec3f, fp: f32, mode: u32) -> vec2f {
  let q = p - a.xyz;
  if (b.y > 0.5) { return birch_d(q, a.w, b.x, b.z, b.w, fp, mode); }
  return spruce_d(q, a.w, b.x, b.z, b.w, fp, mode);
}

/** Soft-shadow estimate with Quílez's closest approach between this sample and the last, so the
 *  penumbra depends less on where the steps happen to land. */
fn soft_acc(res: ptr<function, f32>, h: f32, ph: ptr<function, f32>, t: f32) {
  if (h < 1e-4) { *res = 0.0; return; }
  let y = h * h / (2.0 * *ph);
  let dd = sqrt(max(h * h - y * y, 0.0));
  *res = min(*res, SOFT * dd / max(t - y, 0.05));
  *ph = h;
}

fn top_mode() -> u32 { return select(M_BOUND, M_PROXY, PROXY); }

/** Marches one tree between s0 and s1 (inside its cell). In shadow mode it accumulates the soft
 *  shadow estimate in `res` and returns 0 if fully blocked. */
fn march_tree(a: vec4f, b: vec4f, o: vec3f, d: vec3f, s0: f32, s1: f32, cone: Cone, shadow: bool, res: ptr<function, f32>) -> f32 {
  let birch = b.y > 0.5;
  let disp = select(SP_DISP, BI_DISP, birch);
  let lip = select(SP_L, BI_L, birch);
  // Shadows are cast by the base shapes: true distance bounds, so long steps, and a low sun's ray
  // can cross a forest within its budget. (With leaf displacement, 40 steps ran out at sunset and
  // the capped rays counted as lit: a glint path where the trees hide the sun.)
  var mode = select(top_mode(), M_BASE, shadow);
  var t = s0;
  var ph = 1e10;
  loop {
    if (C.steps >= C.limit) { C.caps += 1u; return INF; }
    C.steps += 1u;
    let p = o + d * t;
    let fp = fp_at(cone, t);
    let eps = max(EPS * fp, 1e-4);
    var dist = tree_eval(a, b, p, fp, mode).x;
    if (mode == M_BOUND && dist < 0.5 * disp + eps) {
      mode = M_FULL;
      dist = tree_eval(a, b, p, fp, mode).x;
    }
    let sc = select(STEP, STEP / lip, mode == M_FULL);
    if (shadow) {
      soft_acc(res, dist, &ph, t);
      if (*res < 0.01) { *res = 0.0; return 0.0; }
      t += max(dist * sc, 0.02);
    } else {
      if (dist < eps) { return t + dist * sc; }   // one more step: onto the surface, no extra evaluation
      t += dist * sc;
    }
    if (t > s1) { return INF; }
  }
}

/** Fake canopy occlusion for ground under a tree: the cell's tree only. */
fn canopy_ao(xz: vec2f) -> f32 {
  let c = vec2i(floor((xz - GRID_ORIGIN) / CELL));
  if (any(c < vec2i(0)) || any(c >= vec2i(i32(GRID_N)))) { return 1.0; }
  let i = u32(c.y) * GRID_N + u32(c.x);
  let a = objs[2u * i];
  if (a.w <= 0.0) { return 1.0; }
  let r = objs[2u * i + 1u].x;
  return mix(0.4, 1.0, smoothstep(0.3, 1.4, distance(xz, a.xz) / (r + 0.4)));
}

// ---- rocks -----------------------------------------------------------------------------------------------

const ROCK_DISP: f32 = 0.1225;                         // 0.07·(1 + 0.5 + 0.25)
const ROCK_L: f32 = 1.0 + 0.07 * 1.6 * 3.0 * NOISE_SLOPE;

fn rock_eval(k: u32, p: vec3f, fp: f32, mode: u32) -> f32 {
  let a = objs[ROCK0 + 2u * k];
  let b = objs[ROCK0 + 2u * k + 1u];
  let q = p - a.xyz;
  var dd = ellipsoid_d(q, a.w * b.xyz);
  if (mode == M_BOUND) {
    dd -= ROCK_DISP * a.w;
  } else if (mode == M_FULL) {
    let s = q / a.w * 1.6 + b.w * 13.0;
    let f0 = 1.6 / a.w;
    dd -= 0.07 * a.w * (band(fp, f0) * tnoise3(s) + 0.5 * band(fp, 2.0 * f0) * tnoise3(s * 2.0 + 7.0)
                        + 0.25 * band(fp, 4.0 * f0) * tnoise3(s * 4.0 + 3.0));
  }
  return dd;
}

fn rock_radius(k: u32) -> f32 {
  let a = objs[ROCK0 + 2u * k];
  let b = objs[ROCK0 + 2u * k + 1u];
  return a.w * (max(b.x, max(b.y, b.z)) + ROCK_DISP) + 0.01;
}

/** The union of the rocks whose bound spheres this ray crosses (a per-ray mask, as in spike 02). */
fn trace_rocks(o: vec3f, d: vec3f, t_min: f32, t_max: f32, cone: Cone, shadow: bool, res: ptr<function, f32>) -> Hit {
  let miss = Hit(INF, K_SKY, 0u);
  let n = F.rocks.x;
  let bx = box_iv(o, d, F.rlo.xyz, F.rhi.xyz);
  let in_group = bx.y >= max(bx.x, t_min) && bx.x <= t_max;
  var mask = 0u;
  var t0 = INF;
  var t1 = -INF;
  for (var k = select(F.rocks.y, 0u, in_group); k < n; k++) {
    let sph = RS[k];
    let iv = sphere_iv(o, d, sph.xyz, sph.w);
    if (iv.y > t_min && iv.x < t_max && iv.x <= iv.y) {
      mask |= 1u << k;
      t0 = min(t0, iv.x);
      t1 = max(t1, iv.y);
    }
  }
  if (mask == 0u) { return miss; }
  var t = max(t0, t_min);
  let te = min(t1, t_max);
  var ph = 1e10;
  var mode = top_mode();
  loop {
    if (C.steps >= C.limit) { C.caps += 1u; return miss; }
    C.steps += 1u;
    let p = o + d * t;
    let fp = fp_at(cone, t);
    let eps = max(EPS * fp, 1e-4);
    var dist = INF;
    var id = 0u;
    var m = mask;
    loop {
      if (m == 0u) { break; }
      let k = firstTrailingBit(m);
      m &= m - 1u;
      let dk = rock_eval(k, p, fp, mode);
      if (dk < dist) { dist = dk; id = k; }
    }
    if (mode == M_BOUND && dist < 0.5 * ROCK_DISP + eps) {
      mode = M_FULL;
      dist = rock_eval(id, p, fp, mode);
      var m2 = mask & ~(1u << id);
      loop {
        if (m2 == 0u) { break; }
        let k = firstTrailingBit(m2);
        m2 &= m2 - 1u;
        let dk = rock_eval(k, p, fp, mode);
        if (dk < dist) { dist = dk; id = k; }
      }
    }
    let sc = select(STEP, STEP / ROCK_L, mode == M_FULL);
    if (shadow) {
      soft_acc(res, dist, &ph, t);
      if (*res < 0.01) { *res = 0.0; return Hit(0.0, K_ROCK, id); }
      t += max(dist * sc, 0.02);
    } else {
      if (dist < eps) { return Hit(t + dist * sc, K_ROCK, id); }
      t += dist * sc;
    }
    if (t > te) { return miss; }
  }
}

// ---- the tower ------------------------------------------------------------------------------------------

const TURRET = vec3f(2.65, 0.0, -0.7);
const WIN_FACE = vec2f(0.34, 0.94);       // toward the lake

fn tower_base() -> vec3f { return objs[TOWER_SLOT].xyz; }

fn windows(q: vec3f) -> f32 {
  var best = INF;
  for (var k = 0u; k < 3u; k++) {
    let ang = select(select(-0.9, 0.75, k == 1u), 0.0, k == 0u);
    let hy = select(select(11.2, 8.3, k == 1u), 4.6, k == 0u);
    let c = cos(ang);
    let s = sin(ang);
    let dir = vec2f(WIN_FACE.x * c - WIN_FACE.y * s, WIN_FACE.x * s + WIN_FACE.y * c);
    let u = dot(q.xz, vec2f(dir.y, -dir.x));
    let w = dot(q.xz, dir);
    best = min(best, max(max(abs(u) - 0.17, abs(q.y - hy) - 0.6), abs(w - 3.1) - 0.7));
  }
  return best;
}

/** (distance, material: 0 stone, 1 roof, 2 inside an opening). */
fn tower_eval(p: vec3f, mode: u32) -> vec2f {
  let q = p - tower_base();
  let rxz = length(q.xz);
  var d = max(rxz - (3.25 - 0.018 * q.y), max(-q.y - 1.5, q.y - 13.2));
  // Corbelled parapet: the 0.75 batter makes the gradient 1.25, hence the 0.8.
  d = min(d, max((rxz - 3.4 + 0.75 * max(13.7 - q.y, 0.0)) * 0.8, max(12.6 - q.y, q.y - 14.5)));
  var mat = 0.0;
  if (mode != M_PROXY) {
    let ang = atan2(q.z, q.x);
    let seg = 6.2831853 / 14.0;
    let a2 = ang - seg * round(ang / seg);
    let mer = max(max(abs(a2) * rxz * 0.98 - 0.42, max(rxz - 3.4, 2.85 - rxz)), max(14.4 - q.y, q.y - 15.3));
    d = min(d, mer);
    d = max(d, -max(rxz - 2.85, 13.9 - q.y));
    let win = -windows(q);
    if (win > d) { d = win; mat = 2.0; }
  }
  let tq = q - TURRET;
  let trx = length(tq.xz);
  let tur = max(trx - 1.1, max(-q.y - 1.5, q.y - 16.6));
  if (tur < d) { d = tur; mat = 0.0; }
  let roof = max((trx - 1.45 * clamp((19.6 - q.y) / 3.0, 0.0, 1.0)) * 0.9, max(16.5 - q.y, q.y - 19.6));
  if (roof < d) { d = roof; mat = 1.0; }
  return vec2f(d, mat);
}

fn trace_tower(o: vec3f, d: vec3f, t_min: f32, t_max: f32, cone: Cone, shadow: bool, res: ptr<function, f32>) -> Hit {
  let miss = Hit(INF, K_SKY, 0u);
  let c = tower_base() + vec3f(0.6, 0.0, -0.15);
  let iv = cyl_iv(o, d, c, 4.3, -1.5, 19.7);
  var t = max(iv.x, t_min);
  let te = min(iv.y, t_max);
  var ph = 1e10;
  if (t >= te) { return miss; }
  let mode = select(M_FULL, M_PROXY, PROXY);
  loop {
    if (C.steps >= C.limit) { C.caps += 1u; return miss; }
    C.steps += 1u;
    let dist = tower_eval(o + d * t, mode).x;
    if (shadow) {
      soft_acc(res, dist, &ph, t);
      if (*res < 0.01) { *res = 0.0; return Hit(0.0, K_TOWER, 0u); }
      t += max(dist * STEP, 0.02);
    } else {
      if (dist < max(EPS * fp_at(cone, t), 1e-4)) { return Hit(t + dist * STEP, K_TOWER, 0u); }
      t += dist * STEP;
    }
    if (t > te) { return miss; }
  }
}

// ---- terrain -----------------------------------------------------------------------------------------------
// Traced and shaded from a heightmap cooked once from world.wgsl's terrain(): 2048² texels over
// 300 m (14.6 cm), bilinear. That's the engine's assumed representation (spike 04's subject); the
// analytic field would cost ~20× more per step.

const HM_HALF: f32 = 150.0;

fn terrain_h(xz: vec2f) -> f32 {
  return textureSampleLevel(hmap, hsamp, (xz + HM_HALF) / (2.0 * HM_HALF), 0.0).x;
}

fn terrain_nh(xz: vec2f, fp: f32) -> vec3f {
  let e = max(0.15, fp);
  let hx = terrain_h(xz + vec2f(e, 0.0)) - terrain_h(xz - vec2f(e, 0.0));
  let hz = terrain_h(xz + vec2f(0.0, e)) - terrain_h(xz - vec2f(0.0, e));
  return normalize(vec3f(-hx, 2.0 * e, -hz));
}
// A heightfield traced with a directional slope bound (spike 02): the gap y − h shrinks along the
// ray at most at −d.y + L·|d.xz|. L is a scoped fact (D-092): 0.72 almost everywhere, 1.6 inside
// two boxes around the stream's valley and the escarpment.

const L_FLAT: f32 = 0.72;
const L_STEEP: f32 = 1.6;
const T_YMAX: f32 = 34.0;
const T_YMIN: f32 = -3.2;
const T_HALF: f32 = 150.0;
const LAKE_DRY_R: f32 = 19.0;    // inside this disc the bed is below −0.16 m

fn slab2(o: vec2f, d: vec2f, lo: vec2f, hi: vec2f) -> vec2f {
  let inv = vec2f(safe_inv(d.x), safe_inv(d.y));
  let a = (lo - o) * inv;
  let b = (hi - o) * inv;
  let t0 = max(min(a.x, b.x), min(a.y, b.y));
  let t1 = min(max(a.x, b.x), max(a.y, b.y));
  return select(vec2f(INF, -INF), vec2f(t0, t1), t1 >= t0);
}

/** The ray's intervals inside the two steep boxes, in the stream's frame (meander included in
 *  the box widths). */
fn steep_iv(o: vec3f, d: vec3f) -> vec4f {
  let q = o.xz - S_MOUTH;
  let os = vec2f(dot(q, S_U), dot(q, S_V));
  let ds = vec2f(dot(d.xz, S_U), dot(d.xz, S_V));
  return vec4f(slab2(os, ds, vec2f(S_FALL - 2.6, -31.0), vec2f(S_FALL + 1.6, 31.0)),
               slab2(os, ds, vec2f(-7.0, -14.5), vec2f(47.0, 14.5)));
}

/** A terrain march in progress, so the walk below can advance it cell by cell. */
struct TM { t: f32, tp: f32, gp: f32, t1: f32, rf: f32, rs: f32, st: vec4f, done: bool, hit: f32 }

fn tm_init(o: vec3f, d: vec3f, t_min: f32, t_max: f32) -> TM {
  var m = TM(0.0, 0.0, INF, 0.0, 0.0, 0.0, vec4f(0.0), true, INF);
  let bx = box_iv(o, d, vec3f(-T_HALF, T_YMIN, -T_HALF), vec3f(T_HALF, T_YMAX, T_HALF));
  var t0 = max(bx.x, t_min);
  let t1 = min(bx.y, t_max);
  let lxz = length(d.xz);
  let rf = -d.y + L_FLAT * lxz;
  let rs = -d.y + L_STEEP * lxz;
  if (t1 < t0 || rs <= 0.0) { return m; }
  // A ray at or above the water, not descending, can't meet the bed inside the lake's disc.
  if (d.y >= 0.0 && o.y > -0.15) {
    let s = o.xz + d.xz * t0;
    let a2 = lxz * lxz;
    if (a2 > 1e-10 && dot(s, s) < LAKE_DRY_R * LAKE_DRY_R) {
      let bq = dot(s, d.xz);
      let cq = dot(s, s) - LAKE_DRY_R * LAKE_DRY_R;
      t0 += (-bq + sqrt(max(bq * bq - a2 * cq, 0.0))) / a2;
    }
  }
  return TM(t0, t0, INF, t1, rf, rs, steep_iv(o, d), false, INF);
}

/** Advances the march until it has evaluated beyond `upto`, found the surface, or left the box.
 *  A step never crosses the surface (the slope bound), so "no hit up to t" holds at every stop. */
fn tm_advance(m: ptr<function, TM>, o: vec3f, d: vec3f, upto: f32, cone: Cone) {
  loop {
    if ((*m).done || (*m).t > upto) { return; }
    if (C.tsteps >= T_STEPS) { C.tcaps += 1u; (*m).done = true; (*m).hit = (*m).t; return; }
    C.tsteps += 1u;
    let t = (*m).t;
    let p = o + d * t;
    let gap = p.y - terrain_h(p.xz);
    if (gap < max(EPS * fp_at(cone, t), 1e-4)) {
      // Secant through the last two samples: the hit lands on the surface.
      var th = t;
      if ((*m).gp < INF && (*m).gp > gap) { th = (*m).tp + (t - (*m).tp) * (*m).gp / ((*m).gp - gap); }
      (*m).done = true;
      (*m).hit = th;
      return;
    }
    let st = (*m).st;
    let steep = (t >= st.x && t <= st.y) || (t >= st.z && t <= st.w);
    var s: f32;
    if (steep) {
      s = gap / (*m).rs * STEP;
    } else {
      var nx = INF;
      if (st.x > t) { nx = st.x; }
      if (st.z > t) { nx = min(nx, st.z); }
      if ((*m).rf <= 0.0) {
        if (nx >= (*m).t1) { (*m).done = true; return; }
        s = nx - t + 1e-3;
      } else {
        s = min(gap / (*m).rf * STEP, nx - t + 1e-3);
      }
    }
    (*m).tp = t;
    (*m).gp = gap;
    (*m).t = t + s;
    if ((*m).t > (*m).t1) { (*m).done = true; return; }
  }
}

fn terrain_trace(o: vec3f, d: vec3f, t_min: f32, t_max: f32, cone: Cone) -> f32 {
  var m = tm_init(o, d, t_min, t_max);
  tm_advance(&m, o, d, INF, cone);
  return m.hit;
}

// ---- the walk: terrain and trees together -----------------------------------------------------------
// One traversal along the ray through the tree grid's 4.5 m cells, near to far. In each cell the
// terrain march is advanced first, then the cell's tree is marched inside its bounding cylinder,
// up to the terrain hit if one was found. Both stop at the first hit, so neither marches past what
// the other found. (Tracing terrain first marched on behind the forest to the far hills: about
// 15 terrain steps per reflection ray.) Blocks of 8×8 cells whose tallest tree the ray passes
// above are skipped whole; a block's top is cooked by place.wgsl.

const BLOCK0: u32 = 8265u;

fn dda_next(o: vec3f, cell: vec2i, pos: vec2<bool>, inv: vec2f) -> vec2f {
  return (vec2f(cell + select(vec2i(0), vec2i(1), pos)) * CELL + GRID_ORIGIN - o.xz) * inv;
}

fn walk(o: vec3f, d: vec3f, t_min: f32, t_max: f32, cone: Cone, shadow: bool, res: ptr<function, f32>, with_terrain: bool) -> Hit {
  var m = TM(0.0, 0.0, INF, 0.0, 0.0, 0.0, vec4f(0.0), true, INF);
  if (with_terrain) { m = tm_init(o, d, t_min, t_max); }
  var lim = t_max;
  var t = t_min;
  var t_end = t_max;
  if (d.y > 1e-5) { t_end = min(t_end, (TREE_YMAX - o.y) / d.y); }
  // No tree reaches into the disc around the lake: start the cells at its edge.
  let a2 = dot(d.xz, d.xz);
  let s0 = o.xz + d.xz * t;
  if (a2 > 1e-10 && dot(s0, s0) < TREE_FREE_R * TREE_FREE_R) {
    let bq = dot(s0, d.xz);
    let cq = dot(s0, s0) - TREE_FREE_R * TREE_FREE_R;
    t += (-bq + sqrt(max(bq * bq - a2 * cq, 0.0))) / a2;
  }
  let pos = d.xz >= vec2f(0.0);
  let sgn = select(vec2i(-1), vec2i(1), pos);
  let inv = vec2f(safe_inv(d.x), safe_inv(d.z));
  let tdel = abs(CELL * inv);
  var cell = vec2i(floor((o.xz + d.xz * t - GRID_ORIGIN) / CELL));
  var tnext = dda_next(o, cell, pos, inv);
  let n = i32(GRID_N);
  loop {
    if (t >= min(t_end, lim) || C.steps >= C.limit) { break; }
    let inside = all(cell >= vec2i(0)) && all(cell < vec2i(n));
    if (!inside && ((cell.x < 0 && sgn.x < 0) || (cell.x >= n && sgn.x > 0) || (cell.y < 0 && sgn.y < 0) || (cell.y >= n && sgn.y > 0))) { break; }
    let t_exit = min(tnext.x, tnext.y);
    var seg_end = t_exit;
    var skip = !inside;
    if (inside) {
      let b = vec2u(cell) / 8u;
      let lo = vec2f(b * 8u) * CELL + GRID_ORIGIN;
      let tb = (select(lo, lo + 8.0 * CELL, pos) - o.xz) * inv;
      let tbx = min(tb.x, tb.y);
      let ymin = o.y + d.y * select(tbx, t, d.y >= 0.0);
      if (ymin > objs[BLOCK0 + b.y * 8u + b.x].x) { skip = true; seg_end = tbx; }
    }
    if (with_terrain) {
      tm_advance(&m, o, d, min(seg_end, lim), cone);
      if (m.done && m.hit < lim) { lim = m.hit; }
    }
    if (!skip) {
      C.cells += 1u;
      let i = u32(cell.y) * GRID_N + u32(cell.x);
      let a = objs[2u * i];
      if (a.w > 0.0) {
        let bb = objs[2u * i + 1u];
        let iv = cyl_iv(o, d, a.xyz, bb.x + 0.42, -0.35, a.w + 0.5);
        let s0t = max(iv.x, t);
        let s1t = min(min(iv.y, t_exit), min(t_end, lim));
        if (s0t < s1t) {
          let th = march_tree(a, bb, o, d, s0t, s1t, cone, shadow, res);
          if (shadow) {
            if (*res <= 0.0) { return Hit(0.0, K_TREE, i); }
          } else if (th < INF) {
            return Hit(th, K_TREE, i);
          }
        }
      }
    }
    if (lim <= seg_end) { break; }
    if (skip && inside) {
      t = seg_end + 1e-3;
      cell = vec2i(floor((o.xz + d.xz * t - GRID_ORIGIN) / CELL));
      tnext = dda_next(o, cell, pos, inv);
    } else {
      t = t_exit;
      if (tnext.x < tnext.y) { cell.x += sgn.x; tnext.x += tdel.x; } else { cell.y += sgn.y; tnext.y += tdel.y; }
    }
  }
  if (with_terrain) {
    tm_advance(&m, o, d, lim, cone);
    if (m.done && m.hit <= lim && m.hit < INF) { return Hit(m.hit, K_TERRAIN, 0u); }
  }
  return Hit(INF, K_SKY, 0u);
}

// ---- the whole world ------------------------------------------------------------------------------------

/** Nearest hit: rocks and the tower first (bound intervals reject most rays at once), then the
 *  walk through terrain and trees, bounded by them. */
fn trace_world(o: vec3f, d: vec3f, t_min: f32, t_max: f32, cone: Cone) -> Hit {
  var best = Hit(INF, K_SKY, 0u);
  var lim = t_max;
  var dummy = 1.0;
  if (PROFILE == 4u) {
    let tt = terrain_trace(o, d, t_min, lim, cone);
    if (tt < lim) { best = Hit(tt, K_TERRAIN, 0u); }
    return best;
  }
  if (PROFILE != 3u) {
    let hr = trace_rocks(o, d, t_min, lim, cone, false, &dummy);
    if (hr.t < lim) { best = hr; lim = hr.t; }
    let hw = trace_tower(o, d, t_min, lim, cone, false, &dummy);
    if (hw.t < lim) { best = hw; lim = hw.t; }
  }
  let hv = walk(o, d, t_min, lim, cone, false, &dummy, true);
  if (hv.t < lim) { best = hv; }
  return best;
}

/** Soft sun visibility from p: rocks, tower and trees (terrain self-shadowing is left out). */
fn sun_vis(p: vec3f, n: vec3f, len: f32, start: f32) -> f32 {
  let o = p + n * 0.04 + F.sun.xyz * start;
  let d = F.sun.xyz;
  let cone = Cone(0.02, 0.004);
  let steps0 = C.steps;
  let limit0 = C.limit;
  let caps0 = C.caps;
  C.limit = C.steps + SH_STEPS;
  var res = 1.0;
  _ = trace_rocks(o, d, 0.0, len, cone, true, &res);
  if (res > 0.0) { _ = trace_tower(o, d, 0.0, len, cone, true, &res); }
  if (res > 0.0) { _ = walk(o, d, 0.0, len, cone, true, &res, false); }
  C.shsteps += C.steps - steps0;
  C.steps = steps0;
  if (STATS && C.caps > caps0) { atomicAdd(&stats[S_SH_CAPS], 1u); }
  C.caps = caps0;
  C.limit = limit0;
  let r = clamp(res, 0.0, 1.0);
  return r * r * (3.0 - 2.0 * r);
}

// ---- shading -----------------------------------------------------------------------------------------------

struct Surf { alb: vec3f, n: vec3f, ao: f32, transl: f32, spec: f32 }

/** The water level at xz: the stream's near the stream, else the lake's. */
fn water_level_at(xz: vec2f) -> f32 {
  let sl = stream_sl(xz);
  if (sl.x > 0.0 && abs(sl.y) < S_HALF + 1.0) { return stream_level(sl.x); }
  return 0.0;
}

fn ground_albedo(p: vec3f, n: vec3f, fp: f32) -> vec3f {
  let xz = p.xz;
  let rn = lake_rn(xz);
  let nz = tnoise2(xz * 0.35) * 0.5 + 0.5;
  let fine = band(fp, 2.0) * tnoise2(xz * 2.0);
  var c = mix(vec3f(0.028, 0.045, 0.016), vec3f(0.060, 0.064, 0.026), nz) * (1.0 + 0.3 * fine);
  c = mix(c, vec3f(0.045, 0.036, 0.025) * (1.0 + 0.25 * fine), smoothstep(1.15, 1.5, rn) * 0.75);
  let wl = water_level_at(xz);
  let above = p.y - wl;
  let peb = 0.5 + 0.5 * band(fp, 5.0) * tnoise2(xz * 5.0 + 9.0);
  let streamside = 1.0 - smoothstep(3.0, 6.0, abs(stream_sl(xz).y));
  let sand = mix(mix(vec3f(0.15, 0.13, 0.10), vec3f(0.07, 0.065, 0.055), smoothstep(0.3, 0.7, peb)), vec3f(0.06, 0.05, 0.04), streamside);
  c = mix(c, sand, (1.0 - smoothstep(0.08, 0.35, above + 0.15 * nz)));
  let rock = vec3f(0.075, 0.07, 0.062) * (0.75 + 0.5 * nz) * (0.85 + 0.3 * band(fp, 3.0) * tnoise2(xz * 3.0 + 1.0));
  c = mix(c, mix(rock, vec3f(0.03, 0.045, 0.018), 0.4 * smoothstep(0.4, 0.7, nz)), smoothstep(0.85, 0.6, n.y));
  c *= 1.0 - 0.45 * (1.0 - smoothstep(0.0, 0.12, above));
  return c;
}

fn stone_albedo(q: vec3f, ang: f32, rad: f32, fp: f32) -> vec3f {
  let row = floor(q.y / 0.42);
  let u = ang * rad / 0.8 + 0.5 * fract(row * 0.5) * 2.0;
  let cell = vec2i(i32(floor(u)), i32(row));
  let fu = fract(u);
  let fv = fract(q.y / 0.42);
  let edge = min(min(fu, 1.0 - fu) * 0.8, min(fv, 1.0 - fv) * 0.42);
  let mortar = band(fp, 3.0) * (1.0 - smoothstep(0.015, 0.05, edge));
  var c = vec3f(0.21, 0.195, 0.17) * (0.7 + 0.5 * h01(cell, 9u)) * (0.85 + 0.3 * band(fp, 2.0) * tnoise3(q * 2.0));
  c = mix(c, vec3f(0.11, 0.10, 0.09), mortar * 0.7);
  let moss = smoothstep(1.8, 0.0, q.y + 0.8 * tnoise3(q * 1.3));
  c = mix(c, vec3f(0.05, 0.07, 0.025), moss * 0.75);
  let streak = smoothstep(13.4, 12.0, q.y) * smoothstep(9.0, 12.5, q.y) * (0.5 + 0.5 * tnoise3(vec3f(ang * 9.0, q.y * 0.3, 0.0)));
  return c * (1.0 - 0.35 * streak);
}

fn rock_albedo(k: u32, p: vec3f, n: vec3f, fp: f32) -> vec3f {
  let a = objs[ROCK0 + 2u * k];
  let b = objs[ROCK0 + 2u * k + 1u];
  var c = vec3f(0.105, 0.10, 0.092) * (0.8 + 0.4 * b.w) * (0.8 + 0.4 * band(fp, 4.0) * tnoise3(p * 4.0));
  c = mix(c, vec3f(0.19, 0.185, 0.16), 0.45 * band(fp, 1.5) * smoothstep(0.25, 0.65, tnoise3(p * 1.5 + 4.0)));
  let moss = smoothstep(0.5, 0.9, n.y) * smoothstep(-0.1, 0.35, tnoise3(p * 0.9 + 3.0));
  c = mix(c, vec3f(0.05, 0.075, 0.025), moss);
  let wl = water_level_at(p.xz);
  let wet = 1.0 - smoothstep(wl - 0.02, wl + 0.18, p.y);
  return c * (1.0 - 0.5 * wet);
}

/** Tetrahedral gradient of the hit object's field. */
fn obj_d(h: Hit, p: vec3f, fp: f32) -> f32 {
  let mode = select(M_FULL, M_PROXY, PROXY);
  if (h.kind == K_TREE) { return tree_eval(objs[2u * h.id], objs[2u * h.id + 1u], p, fp, mode).x; }
  if (h.kind == K_ROCK) { return rock_eval(h.id, p, fp, mode); }
  return tower_eval(p, mode).x;
}

fn obj_normal(h: Hit, p: vec3f, fp: f32) -> vec3f {
  let e = max(0.5 * fp, 0.003);
  let k0 = vec3f(1.0, -1.0, -1.0);
  let k1 = vec3f(-1.0, -1.0, 1.0);
  let k2 = vec3f(-1.0, 1.0, -1.0);
  let k3 = vec3f(1.0, 1.0, 1.0);
  return normalize(k0 * obj_d(h, p + k0 * e, fp) + k1 * obj_d(h, p + k1 * e, fp)
                 + k2 * obj_d(h, p + k2 * e, fp) + k3 * obj_d(h, p + k3 * e, fp));
}

fn surface(h: Hit, p: vec3f, fp: f32) -> Surf {
  var s = Surf(vec3f(0.1), vec3f(0.0, 1.0, 0.0), 1.0, 0.0, 0.03);
  if (h.kind == K_TERRAIN) {
    s.n = terrain_nh(p.xz, fp);
    s.alb = ground_albedo(p, s.n, fp);
    s.ao = canopy_ao(p.xz);
    return s;
  }
  s.n = obj_normal(h, p, fp);
  if (h.kind == K_TREE) {
    let a = objs[2u * h.id];
    let b = objs[2u * h.id + 1u];
    let mode = select(M_FULL, M_PROXY, PROXY);
    let part = tree_eval(a, b, p, fp, mode).y;
    let q = p - a.xyz;
    let birch = b.y > 0.5;
    let tone = 0.75 + 0.5 * fract(b.w * 7.13);
    if (part > 0.5) {
      s.alb = select(vec3f(0.07, 0.05, 0.04), vec3f(0.42, 0.40, 0.37), birch);
      if (birch) { s.alb *= 1.0 - 0.8 * smoothstep(0.35, 0.6, tnoise3(vec3f(q.x * 8.0, q.y * 2.5, q.z * 8.0))) * band(fp, 2.5); }
      s.ao = 0.55;
    } else {
      s.alb = select(vec3f(0.020, 0.042, 0.019), vec3f(0.050, 0.075, 0.022), birch) * tone;
      let y0 = select(0.16 * a.w, 0.3 * a.w, birch);
      let up = clamp((q.y - y0) / (a.w - y0), 0.0, 1.0);
      let outward = clamp(dot(s.n.xz, normalize(q.xz + vec2f(1e-4, 0.0))) * 0.5 + 0.5, 0.0, 1.0);
      s.ao = (0.3 + 0.7 * up) * (0.45 + 0.55 * outward);
      s.transl = select(0.25, 0.6, birch);
    }
    s.spec = 0.02;
  } else if (h.kind == K_TOWER) {
    let mode = select(M_FULL, M_PROXY, PROXY);
    let mat = tower_eval(p, mode).y;
    let q = p - tower_base();
    if (mat > 1.5) {
      s.alb = vec3f(0.03, 0.028, 0.025);
      s.ao = 0.2;
    } else if (mat > 0.5) {
      s.alb = vec3f(0.06, 0.055, 0.06) * (0.85 + 0.3 * band(fp, 6.0) * smoothstep(0.3, 0.7, fract(q.y * 6.0)));
      s.spec = 0.06;
    } else {
      let tq = q - TURRET;
      let on_turret = length(tq.xz) < 1.25 && length(q.xz) > 2.9;
      let ang = select(atan2(q.z, q.x), atan2(tq.z, tq.x), on_turret);
      let rad = select(3.1, 1.1, on_turret);
      s.alb = stone_albedo(q, ang, rad, fp);
      s.ao = select(1.0, 0.75, q.y > 12.5 && q.y < 13.8 && length(q.xz) < 3.45);
    }
  } else if (h.kind == K_ROCK) {
    s.alb = rock_albedo(h.id, p, s.n, fp);
    s.ao = 0.9;
  }
  return s;
}

/** Sun, sky, a little specular, and light through leaves. `sh` is sun visibility. */
fn light(s: Surf, p: vec3f, v: vec3f, sh: f32) -> vec3f {
  let l = F.sun.xyz;
  let ndl = max(dot(s.n, l), 0.0);
  var c = s.alb * (F.sun_col.rgb * ndl * sh + sky_amb(s.n.y) * s.ao);
  c += s.alb * F.sun_col.rgb * s.transl * sh * pow(max(dot(-v, l), 0.0), 3.0) * 0.8;
  let h = normalize(l + v);
  c += F.sun_col.rgb * s.spec * pow(max(dot(s.n, h), 0.0), 40.0) * ndl * sh;
  return c;
}

/** Colour of a hit seen along d, without shadow rays (reflections). The ground under trees is
 *  darkened by the fake canopy term instead. */
fn shade_cheap(h: Hit, o: vec3f, d: vec3f, cone: Cone) -> vec3f {
  let p = o + d * h.t;
  let fp = fp_at(cone, h.t);
  let s = surface(h, p, fp);
  let sh = select(1.0, s.ao, h.kind == K_TERRAIN);
  return light(s, p, -d, sh);
}
