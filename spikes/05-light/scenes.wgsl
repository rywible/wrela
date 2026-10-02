// The three scene fields, their materials, and the marchers that trace them. Appended to
// common.wgsl. Every scene is a heightfield terrain (traced with a directional Lipschitz bound, as in
// spike 02) plus objects (sphere-traced). One marcher steps by the smaller of the two safe steps.
//
// Lighting rays can trace the analytic field, or the cooked field, or a hybrid: analytic for the first
// stretch of each ray (so contact shadows and contact occlusion keep their detail), cooked beyond.
// The cooked field is a terrain height map with its gradient (2D, so terrain keeps the directional
// step) plus a two-level clipmap of the objects' distances (3D), both filled on device from the
// analytic field.

override SCENE: u32 = 0u;          // 0 forest floor, 1 stone tower, 2 golden-hour landscape
override PRIMARY_STEPS: u32 = 256u;
override SUN_STEPS: u32 = 96u;
override SKY_STEPS: u32 = 32u;
override PROBE_STEPS: u32 = 64u;
override PSHADOW_STEPS: u32 = 64u;
override REF_STEPS: u32 = 2000u;     // per reference ray; dispatches cover one tile each

const F_ANALYTIC: u32 = 0u;
const F_COOKED: u32 = 1u;
const F_HYBRID: u32 = 2u;

@group(1) @binding(26) var lin: sampler;
@group(1) @binding(27) var vol_t: texture_3d<f32>;
@group(1) @binding(38) var vol1_t: texture_3d<f32>;
@group(1) @binding(39) var hm_t: texture_2d<f32>;
@group(1) @binding(41) var sh_t: texture_2d<f32>;

// Sun rays: terrain shadows from the cooked shadow-height map (one fetch), objects still traced.
override SUN_TMAP: bool = false;

// ---- per-scene constants ----------------------------------------------------------------------------

/** The terrain's slope bound for the fast marchers. A hypothesis per scene (strict bounds of the
 *  value-noise sums are about 1.7× these); the reference marches with twice it. */
fn t_slope() -> f32 {
  switch SCENE { case 0u: { return 0.45; } case 1u: { return 0.3; } default: { return 1.3; } }
}
/** Step scale for objects: 1 / their Lipschitz bound (displacement, polar boxes, splayed windows). */
fn obj_step() -> f32 {
  switch SCENE { case 0u: { return 0.75; } case 1u: { return 0.85; } default: { return 0.8; } }
}
/** Nothing is above this height: rays going up past it have escaped. */
fn y_top() -> f32 {
  switch SCENE { case 0u: { return 23.5; } case 1u: { return 9.0; } default: { return 260.0; } }
}
/** The terrain is never above this height (so the ground needn't be evaluated above it). */
fn ground_max() -> f32 {
  switch SCENE { case 0u: { return 0.7; } case 1u: { return 0.85; } default: { return 205.0; } }
}
/** Above ground_max() + this margin, the terrain gap is replaced by the cheap bound p.y − ground_max().
 *  The margin exceeds every hit tolerance in use, so the bound can never be mistaken for a surface. */
fn gap_margin() -> f32 {
  switch SCENE { case 0u: { return 1.0; } case 1u: { return 1.0; } default: { return 12.0; } }
}
/** Objects never rise more than this above the terrain under them. */
fn obj_height() -> f32 {
  switch SCENE { case 0u: { return 23.5; } case 1u: { return 9.0; } default: { return 21.0; } }
}
fn world_r() -> f32 {
  switch SCENE { case 0u: { return 400.0; } case 1u: { return 300.0; } default: { return 4000.0; } }
}

// ---- small SDF helpers --------------------------------------------------------------------------------

fn smin(a: f32, b: f32, k: f32) -> f32 {
  let h = max(k - abs(a - b), 0.0) / k;
  return min(a, b) - h * h * k * 0.25;
}
fn box2(q: vec2f, b: vec2f) -> f32 {
  let d = abs(q) - b;
  return length(max(d, vec2f(0.0))) + min(max(d.x, d.y), 0.0);
}
fn box3(q: vec3f, b: vec3f) -> f32 {
  let d = abs(q) - b;
  return length(max(d, vec3f(0.0))) + min(max(d.x, max(d.y, d.z)), 0.0);
}
/** Segment a→b with radius interpolated r1→r2 (a tapered capsule; a bound for small tapers). */
fn taper(p: vec3f, a: vec3f, b: vec3f, r1: f32, r2: f32) -> f32 {
  let pa = p - a;
  let ba = b - a;
  let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
  return length(pa - ba * h) - mix(r1, r2, h);
}
/** Vertical capped cylinder centred at c, radius r, half height hh. */
fn cyl(p: vec3f, c: vec3f, r: f32, hh: f32) -> f32 {
  let q = p - c;
  let d = vec2f(length(q.xz) - r, abs(q.y) - hh);
  return min(max(d.x, d.y), 0.0) + length(max(d, vec2f(0.0)));
}
/** Cone with apex at the origin pointing up (+y), base radius r at depth h below (iq's exact cone). */
fn cone(p: vec3f, r: f32, h: f32) -> f32 {
  let q = vec2f(r, -h);
  let w = vec2f(length(p.xz), p.y);
  let a = w - q * clamp(dot(w, q) / dot(q, q), 0.0, 1.0);
  let b = w - q * vec2f(clamp(w.x / q.x, 0.0, 1.0), 1.0);
  let k = sign(q.y);
  let d = min(dot(a, a), dot(b, b));
  let s = max(k * (w.x * q.y - w.y * q.x), k * (w.y - q.y));
  return sqrt(d) * sign(s);
}
fn byte(h: u32, k: u32) -> f32 { return f32((h >> (8u * k)) & 255u) * (1.0 / 255.0); }
/** Vertical segment from y0 to y1 at (c.x, c.y): distance to it. */
fn vseg(p: vec3f, c: vec2f, y0: f32, y1: f32) -> f32 {
  return length(vec3f(p.x - c.x, p.y - clamp(p.y, y0, y1), p.z - c.y));
}

// =====================================================================================================
// Scene 0: forest floor under a closed canopy. Tall trees on a jittered 8 m grid: a tapered trunk
// with a root flare, clear to 10–14 m, then a crown of twelve displaced blobs. Bushes and mossy rocks
// share the cells. The 2×2 nearest cells are evaluated; everything in a cell lies within 5.8 m of its
// trunk axis, and 5.8 m ≤ the 8 m cell minus the 1 m jitter, so that is exact. A cell is skipped when its
// bounding capsule is farther than the best distance so far.
// =====================================================================================================

const FC: f32 = 8.0;
const FJ: f32 = 1.0;
const CELL_R: f32 = 5.8;
const CROWN_DISP: f32 = 0.45;

fn forest_ground(x: f32, z: f32) -> vec3f {
  let q = vec2f(x, z);
  let a = vnoise2d(q * 0.11);
  let b = vnoise2d(q * 0.37 + 17.0);
  let c = vnoise2d(q * 1.3 + 41.0);
  return vec3f(0.42 * a.x + 0.16 * b.x + 0.05 * c.x,
               0.42 * 0.11 * a.y + 0.16 * 0.37 * b.y + 0.05 * 1.3 * c.y,
               0.42 * 0.11 * a.z + 0.16 * 0.37 * b.z + 0.05 * 1.3 * c.z);
}

fn ball(r: vec3f, c: vec3f, rad: f32) -> f32 { return length(r - c) - rad; }

/** The crown template (12 blobs; reach 4.3 m), in its own rotated, scaled frame. */
fn crown_template(r: vec3f) -> f32 {
  var d = ball(r, vec3f(0.0, -0.6, 0.0), 2.0);
  d = smin(d, ball(r, vec3f(2.6, 0.0, 0.0), 1.7), 0.7);
  d = smin(d, ball(r, vec3f(1.3, 0.2, 2.25), 1.6), 0.7);
  d = smin(d, ball(r, vec3f(-1.3, -0.2, 2.25), 1.75), 0.7);
  d = smin(d, ball(r, vec3f(-2.6, 0.1, 0.0), 1.6), 0.7);
  d = smin(d, ball(r, vec3f(-1.3, 0.0, -2.25), 1.7), 0.7);
  d = smin(d, ball(r, vec3f(1.3, -0.1, -2.25), 1.6), 0.7);
  d = smin(d, ball(r, vec3f(1.15, 1.6, 1.15), 1.6), 0.7);
  d = smin(d, ball(r, vec3f(-1.15, 1.7, 1.15), 1.5), 0.7);
  d = smin(d, ball(r, vec3f(-1.15, 1.5, -1.15), 1.6), 0.7);
  d = smin(d, ball(r, vec3f(1.15, 1.6, -1.15), 1.5), 0.7);
  d = smin(d, ball(r, vec3f(0.0, 2.8, 0.0), 1.5), 0.7);
  return d;
}

struct FTree { base: vec2f, ht: f32, lean: vec2f, r0: f32, cs: f32, rot: vec2f, h: u32, tree: bool }

fn forest_tree(c: vec2i) -> FTree {
  var t: FTree;
  let h = hash2i(c + vec2i(1000, 77));
  let g = ihash(h);
  t.h = h;
  t.tree = byte(h, 0u) < 0.92;
  t.base = (vec2f(c) + 0.5) * FC + (vec2f(byte(h, 1u), byte(h, 2u)) * 2.0 - 1.0) * FJ;
  t.ht = 10.0 + 4.0 * byte(h, 3u);
  t.lean = (vec2f(byte(g, 0u), byte(g, 1u)) * 2.0 - 1.0) * 0.3;
  t.r0 = 0.3 + 0.18 * byte(g, 2u);
  t.cs = 0.92 + 0.16 * byte(g, 3u);
  let a = 6.2831853 * byte(h, 0u) * 9.0;
  t.rot = vec2f(cos(a), sin(a));
  return t;
}

/** One cell's contents: (distance, material id). Ids: 1 trunk, 2 crown, 3 bush, 4 rock. */
fn forest_cell(p: vec3f, c: vec2i, best: f32) -> vec2f {
  let h0 = hash2i(c + vec2i(1000, 77));
  let b0 = (vec2f(c) + 0.5) * FC + (vec2f(byte(h0, 1u), byte(h0, 2u)) * 2.0 - 1.0) * FJ;
  let bound = vseg(p, b0, -1.0, 24.0) - CELL_R;
  if (bound > best) { return vec2f(bound, 0.0); }
  let t = forest_tree(c);
  var d = INF;
  var id = 0.0;
  if (t.tree) {
    // Trunk with a root flare.
    let top = vec3f(t.base.x + t.lean.x, t.ht, t.base.y + t.lean.y);
    let f = max(1.0 - max(p.y, 0.0) / 1.4, 0.0);
    d = taper(p, vec3f(t.base.x, -1.0, t.base.y), top, t.r0, t.r0 * 0.45) - t.r0 * 0.9 * f * f;
    id = 1.0;
    // Crown: bound, then template, then leafy displacement near the surface.
    let cc = vec3f(t.base.x + t.lean.x * 1.3, t.ht + 3.4, t.base.y + t.lean.y * 1.3);
    let q = p - cc;
    let cb = length(q) - 4.4 * t.cs - CROWN_DISP;
    if (cb < min(best, d) && cb < 1.0) {
      let r = vec3f(t.rot.x * q.x - t.rot.y * q.z, q.y, t.rot.y * q.x + t.rot.x * q.z) / t.cs;
      var dc = crown_template(r) * t.cs;
      if (dc < CROWN_DISP + 0.15) {
        let s = vec3f(f32(t.h & 63u));
        dc += CROWN_DISP * vnoise3(q * 0.95 + s);
        // Gaps through the foliage, where a second noise is high: they let the sun through as dapples.
        dc = max(dc, (vnoise3(q * 0.75 + s.zxy + 7.0) - 0.38) * 0.6);
      }
      if (dc < d) { d = dc; id = 2.0; }
    } else if (cb < d) { d = cb; id = 2.0; }
  }
  // Bush (about half the cells) and rock (about a third), near the trunk.
  let hb = ihash(t.h ^ 0xb5u);
  if (byte(hb, 0u) < 0.55) {
    let a = 6.2831853 * byte(hb, 1u);
    let rr = 2.0 + 1.6 * byte(hb, 2u);
    let bc = vec3f(t.base.x + cos(a) * rr, 0.2, t.base.y + sin(a) * rr);
    let bb = length(p - bc) - 1.6;
    if (bb < min(best, d) && bb < 0.8) {
      var db = length(p - bc) - 0.85;
      db = smin(db, length(p - bc - vec3f(0.6, -0.1, 0.3)) - 0.6, 0.4);
      db = smin(db, length(p - bc - vec3f(-0.45, -0.15, 0.5)) - 0.55, 0.4);
      db += 0.2 * vnoise3(p * 2.1);
      if (db < d) { d = db; id = 3.0; }
    } else if (bb < d) { d = bb; id = 3.0; }
  }
  if (byte(hb, 3u) < 0.33) {
    let hr = ihash(hb);
    let a = 6.2831853 * byte(hr, 0u);
    let rr = 1.8 + 1.6 * byte(hr, 1u);
    let rad = 0.45 + 0.7 * byte(hr, 2u);
    let rc = vec3f(t.base.x + cos(a) * rr, -0.25 * rad, t.base.y + sin(a) * rr);
    let dr = (length((p - rc) / vec3f(1.0, 0.62, 1.0)) - rad) * 0.62;
    if (dr < d) { d = dr; id = 4.0; }
  }
  return vec2f(d, id);
}

fn forest_objects(p: vec3f, best_in: f32) -> vec2f {
  let q = p.xz / FC - 0.5;
  let c0 = vec2i(floor(q));
  var best = vec2f(best_in, 0.0);
  for (var i = 0; i < 4; i++) {
    let r = forest_cell(p, c0 + vec2i(i & 1, i >> 1), best.x);
    if (r.x < best.x) { best = r; }
  }
  return best;
}

fn forest_albedo(p: vec3f, n: vec3f, id: f32) -> vec3f {
  if (id < 0.5) {
    // Ground: leaf litter, moss, a little bare soil.
    let m = vnoise2(p.xz * 0.35) + 0.5 * vnoise2(p.xz * 1.1 + 9.0);
    let f = vnoise2(p.xz * 4.0) * 0.5 + 0.5;
    let litter = mix(vec3f(0.13, 0.08, 0.04), vec3f(0.21, 0.13, 0.06), f);
    let moss = mix(vec3f(0.05, 0.10, 0.025), vec3f(0.10, 0.16, 0.035), f);
    return mix(litter, moss, smoothstep(0.15, 0.65, m));
  }
  if (id < 1.5) {
    let s = vnoise3(p * vec3f(7.0, 0.6, 7.0)) * 0.5 + 0.5;
    let bark = mix(vec3f(0.07, 0.06, 0.05), vec3f(0.17, 0.14, 0.11), s);
    return mix(bark, vec3f(0.06, 0.11, 0.03), smoothstep(0.8, 0.0, p.y) * 0.7);
  }
  if (id < 2.5) {
    let v = vnoise3(p * 0.5) * 0.5 + 0.5;
    let w = vnoise3(p * 3.1) * 0.5 + 0.5;
    return mix(vec3f(0.035, 0.08, 0.018), vec3f(0.11, 0.16, 0.03), v) * (0.75 + 0.5 * w);
  }
  if (id < 3.5) {
    let v = vnoise3(p * 2.0) * 0.5 + 0.5;
    return mix(vec3f(0.03, 0.07, 0.02), vec3f(0.07, 0.13, 0.03), v);
  }
  let v = vnoise3(p * 3.0) * 0.5 + 0.5;
  return mix(vec3f(0.22, 0.21, 0.19) * (0.8 + 0.3 * v), vec3f(0.06, 0.12, 0.03), smoothstep(0.5, 0.85, n.y));
}

// =====================================================================================================
// Scene 1: a stone tower. Round, 0.9 m walls (inner radius 3.7 m), three deep splayed windows, a
// timber ceiling at 5 m on joists over a main beam, a central pillar, cantilevered stair steps along
// the wall, a table and barrels. A stone vault at 8 m closes the top, so the floor above is dark too.
// =====================================================================================================

const RI: f32 = 3.7;
const RO: f32 = 4.6;
const RM: f32 = 4.15;
const WH: f32 = 0.45;
const ST0: f32 = 2.75;                      // first stair step's azimuth
const DPHI: f32 = 0.27;
const RISE: f32 = 0.25;
const NSTEP: i32 = 19;

fn tower_ground(x: f32, z: f32) -> vec3f {
  let q = vec2f(x, z);
  let r = max(length(q), 1e-3);
  let t = clamp((r - 7.0) / 9.0, 0.0, 1.0);
  let w = t * t * (3.0 - 2.0 * t);
  let dw = 6.0 * t * (1.0 - t) / 9.0;
  let a = vnoise2d(q * 0.06);
  let b = vnoise2d(q * 0.21 + 5.0);
  let n = 0.6 * a.x + 0.2 * b.x;
  let nx = 0.6 * 0.06 * a.y + 0.2 * 0.21 * b.y;
  let nz = 0.6 * 0.06 * a.z + 0.2 * 0.21 * b.z;
  return vec3f(w * n, w * nx + dw * n * x / r, w * nz + dw * n * z / r);
}

/** A splayed, arched window opening through the wall at azimuth (cos, sin) = er. */
fn window_cut(p: vec3f, er: vec2f) -> f32 {
  let rho = dot(p.xz, er);
  let tau = dot(p.xz, vec2f(-er.y, er.x));
  if (rho < 2.0 || abs(tau) > 2.2) { return 1.0; }
  let s = clamp((rho - RI) / (RO - RI), 0.0, 1.0);
  let w = mix(0.78, 0.36, s);
  let y0 = mix(0.85, 1.1, s);
  let h0 = 1.85;
  let q = vec2f(abs(tau), p.y - y0);
  let d2 = min(box2(q - vec2f(0.0, h0 * 0.5), vec2f(w, h0 * 0.5)), length(q - vec2f(0.0, h0)) - w);
  return max(d2 * 0.75, abs(rho - RM) - WH - 0.3);
}

fn masonry(p: vec3f, r: f32) -> f32 {
  let course = floor(p.y / 0.42);
  let u = atan2(p.z, p.x) * r / 0.75 + 0.5 * (course - 2.0 * floor(course * 0.5));
  let jy = abs(fract(p.y / 0.42 + 0.5) - 0.5) * 0.42;
  let ju = abs(fract(u + 0.5) - 0.5) * 0.75;
  return 0.012 * (1.0 - smoothstep(0.0, 0.03, min(jy, ju)));
}

fn stairs(p: vec3f, r: f32) -> f32 {
  let rb = (RI - 1.1) - r;
  if (rb > 0.3) { return rb; }
  var a = atan2(p.z, p.x) - ST0;
  a = a - 6.2831853 * floor(a / 6.2831853);
  let k0 = i32(floor(a / DPHI));
  var d = INF;
  for (var dk = -1; dk <= 1; dk++) {
    let k = k0 + dk;
    if (k < 0 || k >= NSTEP) { continue; }
    let da = (a - (f32(k) + 0.5) * DPHI) * r;
    let yt = f32(k + 1) * RISE;
    d = min(d, box3(vec3f(da, p.y - (yt - 0.11), r - (RI - 0.5)), vec3f(0.5 * DPHI * r + 0.01, 0.11, 0.6)));
  }
  return d;
}

/** (distance, id). Ids: 1 stone (walls, stairs, pillar), 2 wood, 3 vault. */
fn tower_objects(p: vec3f, best_in: f32) -> vec2f {
  let r = length(p.xz);
  var best = vec2f(best_in, 0.0);
  let bound = max(r - (RO + 0.35), max(p.y - 8.7, -1.2 - p.y));
  if (bound > best.x || bound > 1.0) { return vec2f(bound, 1.0); }
  var dw = max(abs(r - RM) - WH, max(p.y - 8.2, -1.0 - p.y));
  dw = max(dw, -window_cut(p, vec2f(1.0, 0.0)));
  dw = max(dw, -window_cut(p, vec2f(-0.6663, 0.7457)));
  dw = max(dw, -window_cut(p, vec2f(-0.5748, -0.8183)));
  if (abs(dw) < 0.05) { dw += masonry(p, r); }
  if (dw < best.x) { best = vec2f(dw, 1.0); }
  let dv = max(abs(p.y - 8.3) - 0.3, r - RO);
  if (dv < best.x) { best = vec2f(dv, 3.0); }
  if (r < RI + 0.6) {
    var dc = max(abs(p.y - 5.2) - 0.2, r - (RI + 0.3));
    let zj = (fract(p.z / 0.9 + 0.5) - 0.5) * 0.9;
    dc = min(dc, max(box2(vec2f(zj, p.y - 4.88), vec2f(0.11, 0.12)), r - (RI + 0.2)));
    dc = min(dc, max(box2(vec2f(p.x, p.y - 4.6), vec2f(0.17, 0.16)), r - (RI + 0.2)));
    if (dc < best.x) { best = vec2f(dc, 2.0); }
    var dp = cyl(p, vec3f(0.0, 2.3, 0.0), 0.3, 2.3);
    dp = min(dp, cyl(p, vec3f(0.0, 0.15, 0.0), 0.46, 0.15));
    dp = min(dp, cyl(p, vec3f(0.0, 4.35, 0.0), 0.42, 0.1));
    if (dp < best.x) { best = vec2f(dp, 1.0); }
    let ds = stairs(p, r);
    if (ds < best.x) { best = vec2f(ds, 1.0); }
    var dt = box3(p - vec3f(-1.3, 0.78, 1.7), vec3f(0.65, 0.035, 0.38));
    let lq = vec3f(abs(p.x + 1.3) - 0.55, p.y - 0.38, abs(p.z - 1.7) - 0.3);
    dt = min(dt, box3(lq, vec3f(0.04, 0.38, 0.04)));
    dt = min(dt, cyl(p, vec3f(1.6, 0.45, -2.6), 0.32, 0.45) - 0.02);
    dt = min(dt, cyl(p, vec3f(2.2, 0.42, -2.1), 0.3, 0.42) - 0.02);
    if (dt < best.x) { best = vec2f(dt, 2.0); }
  }
  return best;
}

fn tower_albedo(p: vec3f, n: vec3f, id: f32) -> vec3f {
  let r = length(p.xz);
  if (id < 0.5) {
    if (r < RO) {
      let o = vec2f(0.0, 0.5 * floor(p.x / 0.7));
      let c = floor(p.xz / 0.7 + o);
      let g = abs(fract(p.xz / 0.7 + o) - 0.5);
      let joint = smoothstep(0.46, 0.5, max(g.x, g.y));
      let v = u01(hash2i(vec2i(c)));
      return mix(vec3f(0.36, 0.33, 0.29) * (0.75 + 0.45 * v), vec3f(0.24, 0.22, 0.2), joint);
    }
    let m = vnoise2(p.xz * 0.5) * 0.5 + 0.5;
    let grass = mix(vec3f(0.08, 0.13, 0.035), vec3f(0.16, 0.17, 0.06), m);
    return mix(vec3f(0.18, 0.15, 0.11), grass, smoothstep(4.8, 6.0, r));
  }
  if (id < 1.5) {
    let course = floor(p.y / 0.42);
    let u = atan2(p.z, p.x) * max(r, 0.5) / 0.75 + 0.5 * (course - 2.0 * floor(course * 0.5));
    let v = u01(hash2i(vec2i(i32(floor(u)), i32(course))));
    let w = vnoise3(p * 5.0) * 0.5 + 0.5;
    let stone = vec3f(0.46, 0.42, 0.36) * (0.72 + 0.4 * v) * (0.85 + 0.25 * w);
    let jy = abs(fract(p.y / 0.42 + 0.5) - 0.5) * 0.42;
    let ju = abs(fract(u + 0.5) - 0.5) * 0.75;
    let joint = 1.0 - smoothstep(0.012, 0.03, min(jy, ju));
    return mix(stone, vec3f(0.52, 0.5, 0.46), joint * 0.7);
  }
  if (id < 2.5) {
    let g = vnoise3(p * vec3f(1.5, 9.0, 1.5)) * 0.5 + 0.5;
    return mix(vec3f(0.14, 0.085, 0.045), vec3f(0.26, 0.16, 0.085), g);
  }
  return vec3f(0.35, 0.33, 0.3);
}

// =====================================================================================================
// Scene 2: a landscape at golden hour. A meandering valley ~320 m across with 150 m walls, five
// octaves of value-noise relief, conifers on a 26 m grid in patches, boulders on a 37 m grid.
// =====================================================================================================

const LT_C: f32 = 26.0;
const LT_J: f32 = 6.0;
const LB_C: f32 = 37.0;

fn land_ground(x: f32, z: f32) -> vec3f {
  let xc = 70.0 * sin(z * 0.0021) + 28.0 * sin(z * 0.0057 + 1.0);
  let dxc = 70.0 * 0.0021 * cos(z * 0.0021) + 28.0 * 0.0057 * cos(z * 0.0057 + 1.0);
  let u = (x - xc) / 320.0;
  let e = exp(-3.0 * u * u);
  let dp = 150.0 * 6.0 * u * e / 320.0;
  var h = vec3f(150.0 * (1.0 - e) - 10.0, dp, -dp * dxc);
  var amp = 30.0;
  var f = 1.0 / 260.0;
  var o = vec2f(0.0);
  for (var i = 0; i < 5; i++) {
    let n = vnoise2d(vec2f(x, z) * f + o);
    h += vec3f(amp * n.x, amp * f * n.y, amp * f * n.z);
    amp *= 0.42;
    f *= 2.3;
    o += vec2f(17.3, 31.7);
  }
  return h;
}

/** An instance's base height: read from the cooked height map, as an engine would place instances
 *  once at load rather than re-evaluate the terrain for every sample. The primary pass, the lighting
 *  rays and the reference all use it, so they all see the same field. */
fn base_height(b: vec2f) -> f32 {
  let g = (b - F.hm_o.xy) / F.hm_o.z;
  return textureSampleLevel(hm_t, lin, (g + 0.5) / vec2f(textureDimensions(hm_t)), 0.0).r;
}

fn land_tree_mask(b: vec2f) -> f32 {
  return 0.5 + 0.5 * sin(b.x * 0.0061 + 1.3) * sin(b.y * 0.0047 + 0.4) + 0.25 * sin(b.y * 0.019);
}

/** (distance, id). Ids: 1 trunk, 2 needles, 3 boulder. */
fn land_objects(p: vec3f, best_in: f32) -> vec2f {
  var best = vec2f(best_in, 0.0);
  if (p.y > 236.0) { return vec2f(p.y - 224.0, 0.0); }   // tree tops ≤ 224 m; the bound is ≥ 12 m
  let q = p.xz / LT_C - 0.5;
  let c0 = vec2i(floor(q));
  for (var i = 0; i < 4; i++) {
    let c = c0 + vec2i(i & 1, i >> 1);
    let h = hash2i(c + vec2i(311, 7));
    let b = (vec2f(c) + 0.5) * LT_C + (vec2f(byte(h, 0u), byte(h, 1u)) * 2.0 - 1.0) * LT_J;
    let rad = 2.0 + 1.1 * byte(h, 3u);
    let dxz = length(p.xz - b) - rad;
    if (dxz > min(best.x, 3.0)) {
      if (dxz < best.x && byte(h, 2u) <= 0.75 * land_tree_mask(b)) { best = vec2f(dxz, 2.0); }
      continue;
    }
    if (byte(h, 2u) > 0.75 * land_tree_mask(b)) { continue; }
    let ht = 10.0 + 9.0 * byte(ihash(h), 0u);
    let gy = base_height(b);
    let rp = p - vec3f(b.x, gy, b.y);
    var dt = taper(rp, vec3f(0.0, -1.0, 0.0), vec3f(0.0, ht * 0.9, 0.0), 0.32, 0.12);
    var id = 1.0;
    var dn = cone(rp - vec3f(0.0, ht, 0.0), rad * 0.62, ht * 0.42);
    dn = min(dn, cone(rp - vec3f(0.0, ht * 0.78, 0.0), rad * 0.85, ht * 0.42));
    dn = min(dn, cone(rp - vec3f(0.0, ht * 0.55, 0.0), rad, ht * 0.45));
    if (dn < dt) { dt = dn; id = 2.0; }
    if (dt < best.x) { best = vec2f(dt, id); }
  }
  let qb = p.xz / LB_C - 0.5;
  let cb = vec2i(floor(qb));
  for (var i = 0; i < 4; i++) {
    let c = cb + vec2i(i & 1, i >> 1);
    let h = hash2i(c + vec2i(-91, 503));
    if (byte(h, 2u) > 0.35) { continue; }
    let b = (vec2f(c) + 0.5) * LB_C + (vec2f(byte(h, 0u), byte(h, 1u)) * 2.0 - 1.0) * 10.0;
    let rad = 1.4 + 3.0 * byte(h, 3u);
    let dxz = length(p.xz - b) - rad * 1.3 - 0.4;
    if (dxz > min(best.x, 2.0)) {
      if (dxz < best.x) { best = vec2f(dxz, 3.0); }
      continue;
    }
    let gy = base_height(b);
    let rp = (p - vec3f(b.x, gy - 0.3 * rad, b.y)) / vec3f(1.3, 0.8, 1.0);
    var db = (length(rp) - rad) * 0.8;
    if (db < 1.0) { db += 0.35 * vnoise3(p * 0.7); }
    if (db * 0.85 < best.x) { best = vec2f(db * 0.85, 3.0); }
  }
  return best;
}

fn land_albedo(p: vec3f, n: vec3f, id: f32) -> vec3f {
  if (id < 0.5) {
    let m = vnoise2(p.xz * 0.013) * 0.5 + 0.5;
    let f = vnoise2(p.xz * 0.19) * 0.5 + 0.5;
    let grass = mix(vec3f(0.11, 0.14, 0.045), vec3f(0.24, 0.20, 0.09), m) * (0.85 + 0.3 * f);
    let rock = vec3f(0.27, 0.25, 0.23) * (0.8 + 0.3 * f);
    return mix(grass, rock, smoothstep(0.75, 0.6, n.y));
  }
  if (id < 1.5) { return vec3f(0.12, 0.08, 0.05); }
  if (id < 2.5) {
    let v = vnoise3(p * 0.9) * 0.5 + 0.5;
    return mix(vec3f(0.025, 0.055, 0.03), vec3f(0.055, 0.10, 0.04), v);
  }
  let v = vnoise3(p * 1.3) * 0.5 + 0.5;
  return vec3f(0.30, 0.28, 0.25) * (0.75 + 0.4 * v);
}

// =====================================================================================================
// Dispatch on SCENE (a pipeline-overridable constant, so each pipeline holds one scene).
// =====================================================================================================

fn terrain(x: f32, z: f32) -> vec3f {
  switch SCENE {
    case 0u: { return forest_ground(x, z); }
    case 1u: { return tower_ground(x, z); }
    default: { return land_ground(x, z); }
  }
}

fn objects_id(p: vec3f, best: f32) -> vec2f {
  switch SCENE {
    case 0u: { return forest_objects(p, best); }
    case 1u: { return tower_objects(p, best); }
    default: { return land_objects(p, best); }
  }
}

fn objects(p: vec3f) -> f32 { return objects_id(p, INF).x; }

fn albedo(p: vec3f, n: vec3f, id: f32) -> vec3f {
  switch SCENE {
    case 0u: { return forest_albedo(p, n, id); }
    case 1u: { return tower_albedo(p, n, id); }
    default: { return land_albedo(p, n, id); }
  }
}

/** Terrain gap (vertical, y − h) with |∇h|², or a cheap lower bound well above the terrain's maximum. */
fn ground_gap(p: vec3f) -> vec2f {
  if (p.y > ground_max() + gap_margin()) { return vec2f(p.y - ground_max(), 0.0); }
  let h = terrain(p.x, p.z);
  return vec2f(p.y - h.x, dot(h.yz, h.yz));
}

/** A distance estimate for the whole scene (terrain gap converted with the local slope). */
fn sdf(p: vec3f) -> f32 {
  let g = ground_gap(p);
  return min(objects(p), g.x * inverseSqrt(1.0 + g.y));
}

/** Material id at a surface point: 0 terrain, else the object's id. */
fn surface_id(p: vec3f) -> f32 {
  let g = ground_gap(p);
  let o = objects_id(p, INF);
  if (g.x * inverseSqrt(1.0 + g.y) < o.x) { return 0.0; }
  return o.y;
}

fn surface_normal(p: vec3f, e: f32) -> vec3f {
  let g = ground_gap(p);
  let o = objects(p);
  if (g.x * inverseSqrt(1.0 + g.y) < o) {
    let h = terrain(p.x, p.z);
    return normalize(vec3f(-h.y, 1.0, -h.z));
  }
  let k = vec2f(1.0, -1.0);
  return normalize(k.xyy * objects(p + k.xyy * e) + k.yyx * objects(p + k.yyx * e) +
                   k.yxy * objects(p + k.yxy * e) + k.xxx * objects(p + k.xxx * e));
}

// =====================================================================================================
// Marchers.
// =====================================================================================================

/** How fast the terrain gap can close along d, per unit t. ≤ 0: the ray can never reach the terrain. */
fn gap_rate(d: vec3f, slope: f32) -> f32 { return -d.y + slope * length(d.xz); }

struct LD { de: f32, step: f32, eps: f32 }   // distance estimate, safe step along the ray, hit tolerance

/** The analytic field at p for a ray with gap rate `rate`. */
fn ldist_analytic(p: vec3f, rate: f32) -> LD {
  var l: LD;
  let o = objects(p);
  l.de = o;
  l.step = o * obj_step();
  l.eps = 0.0;
  if (rate > 0.0 || p.y < ground_max()) {
    let g = ground_gap(p);
    l.de = min(l.de, g.x * inverseSqrt(1.0 + g.y));
    if (rate > 0.0) { l.step = min(l.step, g.x / rate); }
  }
  return l;
}

/** Terrain (height, ∂h/∂x, ∂h/∂z) from the cooked height map (bilinear), or analytic outside it. */
fn terrain_cooked(x: f32, z: f32) -> vec3f {
  let g = (vec2f(x, z) - F.hm_o.xy) / F.hm_o.z;
  if (all(g > vec2f(0.0)) && all(g < vec2f(F.hm_n.xy) - 1.0)) {
    return textureSampleLevel(hm_t, lin, (g + 0.5) / vec2f(textureDimensions(hm_t)), 0.0).rgb;
  }
  return terrain(x, z);
}

/** The cooked field at p: objects from the clipmap's fine level where it covers p, else its coarse
 *  level, else the analytic objects; terrain from the height map. Trilinear distances are a little
 *  optimistic near surfaces, so object steps are scaled by 0.9. */
fn ldist_cooked(p: vec3f, rate: f32, terr: bool) -> LD {
  var l: LD;
  let g0 = (p - F.vol_o.xyz) / F.vol_o.w;
  let g1 = (p - F.vol1_o.xyz) / F.vol1_o.w;
  if (all(g0 > vec3f(0.5)) && all(g0 < vec3f(F.vol_n.xyz) - 0.5)) {
    let v = textureSampleLevel(vol_t, lin, g0 / vec3f(textureDimensions(vol_t)), 0.0).r;
    l = LD(v, v * 0.9, 0.35 * F.vol_o.w);
  } else if (all(g1 > vec3f(0.5)) && all(g1 < vec3f(F.vol1_n.xyz) - 0.5)) {
    let v = textureSampleLevel(vol1_t, lin, g1 / vec3f(textureDimensions(vol1_t)), 0.0).r;
    l = LD(v, v * 0.9, 0.35 * F.vol1_o.w);
  } else {
    let o = objects(p);
    l = LD(o, o * obj_step(), 0.0);
  }
  if (terr && (rate > 0.0 || p.y < ground_max())) {
    var gap = p.y - ground_max();
    var g2 = 0.0;
    if (p.y < ground_max() + gap_margin()) {
      let h = terrain_cooked(p.x, p.z);
      gap = p.y - h.x;
      g2 = dot(h.yz, h.yz);
    }
    l.de = min(l.de, gap * inverseSqrt(1.0 + g2));
    if (rate > 0.0) { l.step = min(l.step, gap / rate); }
  }
  return l;
}

/** Lighting rays' field: analytic, cooked, or hybrid (analytic while t < near, cooked beyond).
 *  `terr` false leaves the terrain out of the cooked part (the shadow-height map covers it). */
fn ldist(p: vec3f, rate: f32, mode: u32, t: f32, near: f32, terr: bool) -> LD {
  if (mode == F_ANALYTIC || (mode == F_HYBRID && t < near)) { return ldist_analytic(p, rate); }
  return ldist_cooked(p, rate, terr);
}

/** The terrain's sun visibility at p from the cooked shadow-height map: the height a point there
 *  must clear to see the sun's centre over the terrain, and the distance to the occluding terrain,
 *  which sets the penumbra's width (two-sided, mapped as in cone_trace). */
fn shadow_height(p: vec3f) -> vec2f {
  let g = (p.xz - F.hm_o.xy) / F.hm_o.z;
  if (any(g < vec2f(0.0)) || any(g > vec2f(F.hm_n.xy) - 1.0)) { return vec2f(-1e9, 0.0); }
  return textureSampleLevel(sh_t, lin, (g + 0.5) / vec2f(textureDimensions(sh_t)), 0.0).rg;
}

fn terrain_sun(p: vec3f) -> f32 {
  let sh = shadow_height(p);
  if (sh.y <= 0.0) { return 1.0; }
  let ce = length(F.sun.xz);
  let res = clamp((p.y - sh.x) * ce * ce / (sh.y * F.sun.w), -1.0, 1.0);
  return 0.25 * (1.0 + res) * (1.0 + res) * (2.0 - res);
}

struct Cone { vis: f32, steps: u32, cap: bool }

/** Distance-field cone trace: visibility of a cone of half-angle width w (radius w·t at distance t)
 *  along d from o, up to tmax. Tracks the smallest h/(w·t).
 *  two_sided (the sun): negative h inside occluders counts, so the penumbra straddles the shadow edge
 *  as an area light's does; [-1, 1] maps to [0, 1] with a smoothstep.
 *  one-sided (sky cones): h is clamped at 0, so a cone whose axis meets a surface is fully blocked,
 *  however thin the surface; [0, 1] maps with a smoothstep. */
fn cone_trace(o: vec3f, d: vec3f, w: f32, t0: f32, tmax: f32, cap: u32, mode: u32, two_sided: bool, near: f32, terr: bool) -> Cone {
  let rate = gap_rate(d, t_slope());
  var res = 1.0;
  var t = t0;
  var c = Cone(1.0, 0u, false);
  var i = 0u;
  let lo = select(0.0, -1.0, two_sided);
  // Minimum step, as a fraction of the cone's radius: wide one-sided cones only need an occlusion
  // estimate, not an intersection, so they stride; the sun's narrow cone creeps.
  let kmin = select(0.5, 0.35, two_sided);
  loop {
    if (i >= cap) { c.cap = true; break; }
    let p = o + d * t;
    if (p.y > y_top() && d.y > 0.0) { break; }
    // A sun ray (terrain from the shadow-height map) that is more than the object layer's height
    // above the height needed to clear all terrain toward the sun can never come near an object again.
    if (!terr && t > near && p.y - shadow_height(p).x > obj_height()) { break; }
    let l = ldist(p, rate, mode, t, near, terr);
    i++;
    res = min(res, l.de / (w * t));
    if (res <= lo) { res = lo; break; }
    t += clamp(l.step, max(kmin * w * t, 0.002), 1e4);
    if (t > tmax) { break; }
  }
  c.steps = i;
  if (two_sided) {
    res = max(res, -1.0);
    c.vis = 0.25 * (1.0 + res) * (1.0 + res) * (2.0 - res);
  } else {
    let r = clamp(res, 0.0, 1.0);
    c.vis = r * r * (3.0 - 2.0 * r);
  }
  return c;
}

struct Hit { t: f32, status: u32, steps: u32 }   // status: 0 escaped, 1 hit, 2 step cap

/** Sphere-traces to the first surface, for probe rays (coarse tolerance). */
fn march_hit(o: vec3f, d: vec3f, tmax: f32, cap: u32, mode: u32, eps_rel: f32, eps_abs: f32, near: f32) -> Hit {
  let rate = gap_rate(d, t_slope());
  var t = 0.0;
  var h = Hit(INF, 0u, 0u);
  var i = 0u;
  loop {
    if (i >= cap) { h.status = 2u; h.t = t; break; }
    let p = o + d * t;
    if ((p.y > y_top() && d.y >= 0.0) || length(p.xz) > world_r()) { break; }
    let l = ldist(p, rate, mode, t, near, true);
    i++;
    let eps = max(max(eps_rel * t, eps_abs), l.eps);
    if (l.de < eps) { h.status = 1u; h.t = t; break; }
    t += max(l.step, eps * 0.5);
    if (t > tmax) { break; }
  }
  h.steps = i;
  return h;
}

/** Primary visibility: the analytic field with a quarter-pixel tolerance, plus a secant step on
 *  terrain hits so they land on the surface. */
fn march_primary(o: vec3f, d: vec3f, pix: f32) -> Hit {
  let rate = gap_rate(d, t_slope());
  var t = 0.0;
  var tp = 0.0;
  var gp = INF;
  var h = Hit(INF, 0u, 0u);
  var i = 0u;
  loop {
    if (i >= PRIMARY_STEPS) { h.status = 2u; h.t = t; break; }
    let p = o + d * t;
    if ((p.y > y_top() && d.y >= 0.0) || length(p.xz) > world_r()) { break; }
    i++;
    let eps = max(0.25 * pix * t, 2e-4);
    let ob = objects(p);
    var g = INF;
    if (rate > 0.0 || p.y < ground_max()) { g = ground_gap(p).x; }
    if (ob < eps) { h.status = 1u; h.t = t; break; }
    if (g < eps) {
      if (gp < INF && gp > g) { t += g * (t - tp) / (gp - g); }
      h.status = 1u;
      h.t = t;
      break;
    }
    var s = ob * obj_step();
    if (rate > 0.0) { s = min(s, g / rate); }
    tp = t;
    gp = g;
    t += max(s, eps * 0.5);
    if (t > 1e5) { break; }
  }
  h.steps = i;
  return h;
}

/** A lower bound on the objects' field at p from the cooked clipmap (−1 outside it). Trilinear
 *  interpolation of samples of a field F with Lipschitz bound L (whose samples may themselves be lower
 *  bounds of F) is at most F(p) + L·√3·voxel; r16float rounding and filter-weight quantization add
 *  at most a fraction of a percent, covered by the extra margins. */
fn cooked_lower(p: vec3f) -> f32 {
  let lip = 1.0 / obj_step();
  let g0 = (p - F.vol_o.xyz) / F.vol_o.w;
  if (all(g0 > vec3f(0.5)) && all(g0 < vec3f(F.vol_n.xyz) - 0.5)) {
    let v = textureSampleLevel(vol_t, lin, g0 / vec3f(textureDimensions(vol_t)), 0.0).r;
    return v - lip * 1.7321 * 1.02 * F.vol_o.w - 0.002 * abs(v);
  }
  let g1 = (p - F.vol1_o.xyz) / F.vol1_o.w;
  if (all(g1 > vec3f(0.5)) && all(g1 < vec3f(F.vol1_n.xyz) - 0.5)) {
    let v = textureSampleLevel(vol1_t, lin, g1 / vec3f(textureDimensions(vol1_t)), 0.0).r;
    return v - lip * 1.7321 * 1.02 * F.vol1_o.w - 0.002 * abs(v);
  }
  return -1.0;
}

/** The reference's marcher: analytic field, half-size steps, twice the terrain slope bound, a tight
 *  tolerance and a step cap in the thousands. In empty space it steps by the cooked lower bound
 *  instead of evaluating the objects; hits are only ever decided by the analytic field. Returns the
 *  hit distance or INF. */
fn march_ref(o: vec3f, d: vec3f, tmax: f32, steps: ptr<function, u32>, caps: ptr<function, u32>) -> f32 {
  let rate = gap_rate(d, 2.0 * t_slope());
  var t = 0.0;
  for (var i = 0u; i < REF_STEPS; i++) {
    let p = o + d * t;
    if ((p.y > y_top() && d.y >= 0.0) || length(p.xz) > world_r()) { *steps += i; return INF; }
    var ob = cooked_lower(p);
    if (ob < 0.05) { ob = objects(p); }
    var g = INF;
    if (rate > 0.0 || p.y < ground_max()) { g = ground_gap(p).x; }
    let eps = max(2e-5 * t, 5e-5);
    if (ob < eps || g < eps) { *steps += i; return t; }
    var s = ob * obj_step() * 0.5;
    if (rate > 0.0) { s = min(s, 0.5 * g / rate); }
    t += max(s, eps * 0.5);
    if (t > tmax) { *steps += i; return INF; }
  }
  *steps += REF_STEPS;
  *caps += 1u;
  return t;
}
