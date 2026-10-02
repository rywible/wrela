// The clearing as one field: terrain, trees on a jittered grid, a stone tower, boulders and the wolf.
// Appended after common.wgsl, the placement block main.js generates, and wolf.wgsl.
//
// Every object returns a Hit: d is its distance (a bound, used for hits, lines, AO and normals),
// s is the safe step (d shrunk by the object's displacement Lipschitz estimate, where it applies).

@group(0) @binding(1) var<storage, read> trees: array<vec4f>;

override CONTENT: u32 = 0u;       // 0 detailed, 1 simple (what a stylized look lets content drop)
override LIP_SCALE: f32 = 0.35;   // share of the strict displacement Lipschitz bound the fast march
                                  // trusts. A hypothesis, checked against the reference (1 = strict).

const MAT_SKY: u32 = 0u;
const MAT_GROUND: u32 = 1u;
const MAT_TRUNK: u32 = 2u;
const MAT_LEAF: u32 = 3u;
const MAT_STONE: u32 = 4u;
const MAT_ROCK: u32 = 5u;
const MAT_WOLF: u32 = 6u;

struct Hit { d: f32, s: f32, mat: u32 }

fn hit_min(a: Hit, b: Hit) -> Hit {
  var r = a;
  if (b.d < a.d) { r.d = b.d; r.mat = b.mat; }
  r.s = min(a.s, b.s);
  return r;
}

const BOUND_SKIP: f32 = 0.3;      // evaluate an object's inner field within this of its bound

// ---- terrain (main.js has the same function, and measures T_SLOPE from it) ----------------------

fn terrain_h(x: f32, z: f32) -> f32 {
  let r = sqrt(x * x + z * z);
  var h = (0.18 * sin(0.21 * x + 0.5) * cos(0.17 * z) + 0.10 * sin(0.43 * x + 1.0) * sin(0.37 * z + 1.0))
        * mix(0.15, 1.0, smoothstep(8.0, 20.0, r));
  h += 4.0 * smoothstep(16.0, 85.0, r);
  // the far terms are zero inside their radii: skip them there (same values)
  if (r > 110.0) { h += 22.0 * smoothstep(110.0, 300.0, r) * (0.55 + 0.45 * sin(0.012 * x + 0.7) * cos(0.010 * z - 0.4)); }
  if (r > 60.0) { h += 4.0 * sin(0.035 * x + 2.0) * sin(0.03 * z) * smoothstep(60.0, 150.0, r); }
  return h;
}

fn terrain_n(x: f32, z: f32, e: f32) -> vec3f {
  return normalize(vec3f(terrain_h(x - e, z) - terrain_h(x + e, z), 2.0 * e, terrain_h(x, z - e) - terrain_h(x, z + e)));
}

// ---- terrain slope zones: the directional bound uses the steepest slope of the zone the ray is in --

struct Zone { s: f32, exit: f32 }

/** The slope bound where p is, and how far along the ray it holds (zones are circles about the origin). */
fn slope_zone(p: vec3f, d: vec3f) -> Zone {
  let r2 = dot(p.xz, p.xz);
  var R = 0.0;
  var z = Zone(T_SLOPE, INF);
  if (r2 < ZONE_R1 * ZONE_R1) { R = ZONE_R1; z.s = ZONE_S1; }
  else if (r2 < ZONE_R2 * ZONE_R2) { R = ZONE_R2; z.s = ZONE_S2; }
  else { return z; }
  let a = max(dot(d.xz, d.xz), 1e-12);
  let b = dot(p.xz, d.xz);
  z.exit = (-b + sqrt(max(b * b - a * (r2 - R * R), 0.0))) / a + 0.05;
  return z;
}

// ---- trees: one per 8 m cell of a grid turned by GRID_A, records in a storage buffer (main.js) ----
// Record (10 vec4s): 0 base xyz, height (0: no tree) | 1 trunk top xyz, top radius | 2 canopy bound
// centre, radius | 3–7 clumps xyz, radius | 8 seed, hue, base radius, branch height (0–1) |
// 9 x: lower bound on the distance from anywhere in this cell to any other cell's tree

const CELL: f32 = 8.0;
const GRID_N: i32 = 22;
const GRID_O: f32 = -88.0;
const TREE_REC: u32 = 10u;
const TREE_TOP: f32 = 12.5;       // above the terrain, no tree reaches this
const TOP_ALL: f32 = 14.0;        // nor does anything else

fn to_grid(v: vec2f) -> vec2f { return vec2f(GRID_C * v.x - GRID_S * v.y, GRID_S * v.x + GRID_C * v.y); }

// Odd rows are shifted by half a cell, so the trees don't line up in rows.
fn grid_cell(g: vec2f) -> vec2i {
  let row = i32(floor((g.y - GRID_O) / CELL));
  let shift = select(0.0, 0.5 * CELL, (row & 1) == 1);
  return vec2i(i32(floor((g.x - GRID_O - shift) / CELL)), row);
}

fn tree_index(p: vec3f) -> i32 {
  let c = grid_cell(to_grid(p.xz));
  if (any(c < vec2i(0)) || any(c >= vec2i(GRID_N))) { return -1; }
  return c.y * GRID_N + c.x;
}

/** Distance along the ray to the current cell's exit, so a step never skips a neighbour's tree. */
fn cell_exit(p: vec3f, d: vec3f) -> f32 {
  let g = to_grid(p.xz);
  let dg = to_grid(d.xz);
  let c = grid_cell(g);
  let lo = vec2f(f32(c.x) * CELL + GRID_O + select(0.0, 0.5 * CELL, (c.y & 1) == 1), f32(c.y) * CELL + GRID_O);
  let wall = select(lo, lo + CELL, dg > vec2f(0.0));
  let dd = select(dg, vec2f(1e-9), abs(dg) < vec2f(1e-9));
  let t = (wall - g) / dd;
  return min(select(t.x, INF, abs(dg.x) < 1e-9), select(t.y, INF, abs(dg.y) < 1e-9));
}

/** A safe step with respect to every tree outside the current cell: the cell's exit, or the cell's
 *  precomputed clearance, whichever is longer (both are safe, so the longer one is). */
fn tree_limit(p: vec3f, d: vec3f, idx: i32) -> f32 {
  var clear = 0.0;
  if (idx >= 0) { clear = trees[u32(idx) * TREE_REC + 9u].x; }
  else {
    // outside the grid: distance to its bounding square (rows reach half a cell further in x)
    let g = abs(to_grid(p.xz)) + vec2f(GRID_O - 0.5 * CELL, GRID_O);
    clear = length(max(g, vec2f(0.0)));
  }
  return max(cell_exit(p, d) + 0.02, clear);
}

// Canopy displacement. Detailed: lumps plus leaf clusters, each faded with the pixel footprint
// (D-077's filtering). Simple: one low-frequency octave, for soft clumps.
const LEAF_AMP_D: f32 = 0.30;
const LEAF_AMP_S: f32 = 0.14;
const LEAF_LIP_S: f32 = 0.95;     // 5.2 × 0.14 × 1.3, strict

fn leaf_disp(p: vec3f, fp: f32) -> f32 {
  if (CONTENT == 1u) { return 0.14 * noise3(p * 1.3 + 3.7); }
  // each octave fades out once its wavelength is under ~4 px (0.18 m and 0.55 m)
  let w1 = 1.0 - smoothstep(0.07, 0.14, fp);
  let w2 = 1.0 - smoothstep(0.02, 0.045, fp);
  var s = 0.20 * w1 * noise3(p * 1.8);
  if (w2 > 0.0) { s += 0.10 * w2 * noise3(p * 5.5 + 11.3); }
  return s;
}
fn leaf_amp() -> f32 { return select(LEAF_AMP_D, LEAF_AMP_S, CONTENT == 1u); }
/** Step scale inside the displaced shell; the faded octave no longer counts toward the slope. */
fn leaf_k(fp: f32) -> f32 {
  if (CONTENT == 1u) { return 1.0 / (1.0 + LIP_SCALE * LEAF_LIP_S); }
  let w1 = 1.0 - smoothstep(0.07, 0.14, fp);
  let w2 = 1.0 - smoothstep(0.02, 0.045, fp);
  return 1.0 / (1.0 + LIP_SCALE * 5.2 * (0.20 * 1.8 * w1 + 0.10 * 5.5 * w2));
}

fn canopy_base(b: u32, p: vec3f) -> f32 {
  var base = sd_sphere(p - trees[b + 3u].xyz, trees[b + 3u].w);
  for (var i = 4u; i < 8u; i++) {
    let c = trees[b + i];
    base = smin(base, sd_sphere(p - c.xyz, c.w), 0.7);
  }
  return base;
}

fn tree_hit(p: vec3f, idx: i32, fp: f32, sw: f32) -> Hit {
  var h = Hit(INF, INF, MAT_LEAF);
  if (idx < 0) { return h; }
  let b = u32(idx) * TREE_REC;
  let r0 = trees[b];
  if (r0.w <= 0.0) { return h; }
  let r1 = trees[b + 1u];
  let misc = trees[b + 8u];
  let c1 = trees[b + 4u];
  let c2 = trees[b + 5u];
  // wood: trunk and two branches, inside a capsule around the trunk that reaches the branch tips
  let ba = r1.xyz - r0.xyz;
  let pa = p - r0.xyz;
  let wb = length(pa - ba * clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0)) - 1.6 * misc.z / 0.30;
  var dw = wb;
  if (wb < BOUND_SKIP + sw) {
    let bp = mix(r0.xyz, r1.xyz, misc.w);
    dw = sd_round_cone(p, r0.xyz, r1.xyz, misc.z, r1.w);
    dw = min(dw, sd_round_cone(p, bp, mix(bp, c1.xyz, 0.8), 0.10, 0.04));
    dw = min(dw, sd_round_cone(p, bp + ba * 0.15, mix(bp, c2.xyz, 0.8), 0.09, 0.035));
  }
  // canopy: bound sphere, then base clumps, then the displaced shell
  let bnd = trees[b + 2u];
  var dl = length(p - bnd.xyz) - bnd.w;
  var kl = 1.0;
  if (dl < BOUND_SKIP + sw) {
    let base = canopy_base(b, p);
    let amp = leaf_amp();
    if (base - amp > sw) { dl = base - amp; }
    else { dl = base - leaf_disp(p, fp); kl = leaf_k(fp); }
  }
  let s = min(dw, dl * kl);
  if (dw < dl) { return Hit(dw, s, MAT_TRUNK); }
  return Hit(dl, s, MAT_LEAF);
}

fn tree_seed(p: vec3f) -> vec4f {
  let i = tree_index(p);
  if (i < 0) { return vec4f(0.0); }
  return trees[u32(i) * TREE_REC + 8u];
}

// ---- the tower -----------------------------------------------------------------------------------

fn wrap_a(a: f32) -> f32 { return a - 2.0 * PI * round(a / (2.0 * PI)); }

/** Masonry courses in (arc, height): x = distance to the nearest joint (m), y = the block's hash. */
fn masonry(q: vec3f, ang: f32) -> vec2f {
  let s = ang * 2.5;
  let row = floor(q.y / 0.42);
  let s2 = s + 0.39 * (row - 2.0 * floor(row * 0.5));
  let col = floor(s2 / 0.78);
  let fx = (s2 / 0.78 - col) * 0.78;
  let fy = (q.y / 0.42 - row) * 0.42;
  let e = min(min(fx, 0.78 - fx), min(fy, 0.42 - fy));
  return vec2f(e, fv_hash(vec3i(i32(row), i32(col), 7)) * 0.5 + 0.5);
}

const MASON_LIP: f32 = 1.14;      // groove 0.028 over 0.05 m, bulge 0.016 over 0.08 m (smoothstep slope 1.5)

/** A door or slit in (arc, height): a rectangle, optionally with a round arch on top. */
fn opening(u: f32, y: f32, half_w: f32, y0: f32, y1: f32, arch: bool) -> f32 {
  let rect = max(abs(u) - half_w, max(y0 - y, y - y1));
  if (arch) { return min(rect, length(vec2f(u, y - y1)) - half_w); }
  return rect;
}

fn tower_hit(p: vec3f, sw: f32) -> Hit {
  let q = p - TOWER_P;
  let rxz = length(q.xz);
  let bound = max(rxz - 3.0, max(-0.8 - q.y, q.y - 12.8));
  if (bound > BOUND_SKIP + sw) { return Hit(bound, bound, MAT_STONE); }
  let R = 2.55 - 0.022 * clamp(q.y, 0.0, 11.0);
  var d = max(rxz - R, max(-q.y - 0.8, q.y - 11.4));
  d = min(d, max(rxz - 2.78, max(-q.y - 0.8, q.y - 0.45)));           // plinth
  d = smin(d, max(rxz - 2.85, max(10.8 - q.y, q.y - 12.6)), 0.3);     // corbelled parapet
  d = max(d, -max(2.32 - rxz, 11.95 - q.y));                          // hollow top
  let ang = wrap_a(atan2(q.z, q.x) - DOOR_A);                         // 0 at the door, seam at the back
  let sec = 2.0 * PI / 14.0;
  let arc = abs(ang - sec * round(ang / sec)) * rxz;
  d = max(d, -max(arc - 0.30, 12.05 - q.y));                          // crenels
  // the door and three arrow slits, recessed 0.45 m
  let u = ang * R;
  var cut = max(opening(u, q.y, 0.62, -0.2, 1.65, true), (R - 0.45) - rxz);
  cut = min(cut, max(opening(wrap_a(ang - 0.9) * R, q.y, 0.09, 3.7, 4.7, false), (R - 0.5) - rxz));
  cut = min(cut, max(opening(wrap_a(ang + 1.1) * R, q.y, 0.09, 6.4, 7.4, false), (R - 0.5) - rxz));
  cut = min(cut, max(opening(wrap_a(ang - 0.25) * R, q.y, 0.09, 8.6, 9.6, false), (R - 0.5) - rxz));
  d = max(d, -cut);
  var k = 1.0;
  if (CONTENT == 0u && q.y < 12.0) {
    // mortar grooves and block bulges, traced in a thin shell around the base surface
    if (d > 0.03 + sw) { d -= 0.016; }
    else {
      let m = masonry(q, ang);
      d += 0.028 * (1.0 - smoothstep(0.0, 0.05, m.x)) - smoothstep(0.02, 0.10, m.x) * (0.006 + 0.010 * m.y);
      k = 1.0 / (1.0 + LIP_SCALE * MASON_LIP);
    }
  }
  return Hit(d, d * k, MAT_STONE);
}

// ---- boulders ---------------------------------------------------------------------------------------

fn sd_rock(q: vec3f, r: vec3f) -> f32 {
  let k0 = length(q / r);
  if (k0 < 1.0) { return (k0 - 1.0) * min(r.x, min(r.y, r.z)); }
  return k0 * (k0 - 1.0) / length(q / (r * r));
}

fn rock_hit(p: vec3f, sw: f32) -> Hit {
  var h = Hit(INF, INF, MAT_ROCK);
  for (var i = 0u; i < N_ROCKS; i++) {
    let c = ROCKS[i];
    let rr = ROCK_R[i];
    let bd = length(p - c.xyz) - c.w;
    if (bd > BOUND_SKIP + sw) { h = hit_min(h, Hit(bd, bd, MAT_ROCK)); continue; }
    let q = rot_y(p - c.xyz, rr.w);
    var d = sd_rock(q, rr.xyz);
    var k = 1.0;
    if (CONTENT == 0u) { d -= 0.06 * noise3(p * 2.3 + f32(i) * 7.0); k = 1.0 / (1.0 + LIP_SCALE * 0.72); }
    else { d -= 0.03 * noise3(p * 1.1 + f32(i) * 7.0); k = 1.0 / (1.0 + LIP_SCALE * 0.17); }
    h = hit_min(h, Hit(d, d * k, MAT_ROCK));
  }
  return h;
}

// ---- the wolf ---------------------------------------------------------------------------------------

override EXACT_WOLF: bool = false;  // the reference evaluates wolf_field as authored, unbounded
var<private> g_coarse: bool = false; // AO-scale queries: the wolf's millimetre fur doesn't matter there

fn wolf_local(p: vec3f) -> vec3f { return rot_y(p - WOLF_P, -WOLF_YAW); }

fn wbox(p: vec3f, lo: vec3f, hi: vec3f) -> f32 { return sd_box(p - 0.5 * (lo + hi), 0.5 * (hi - lo)); }

/** wolf_field with part-group bounds. The authored field is one smooth-union chain: torso, neck,
 *  head, forelegs, hindlegs, tail. A group whose bound is at least the running value plus its blend
 *  radius leaves the chain unchanged (smin(a, b, k) = a when b ≥ a + k), so skipping it is exact,
 *  provided each group lies inside its box. The boxes come from the joint list and part radii, with
 *  ~3 cm to spare; the reference uses wolf_field itself, so a wrong box shows up there. The fur and
 *  tufts move the surface by at most 0.9 × (0.0045 × 0.875 + 0.010), so beyond that shell a bound
 *  stands in for them. */
fn wolf_fast(p: vec3f, sw: f32) -> f32 {
  let b = wolf_body(p);
  let d = b.x;
  let dh = b.y;
  if (d > 0.03 + select(sw, min(sw, 0.05), g_coarse)) { return 0.9 * (d - 0.014); }
  // the authored fur, belly fringe and britches (wolf_field's last lines)
  let fur = wfbm(p * vec3f(34.0, 30.0, 13.0), 3);
  let amp = 0.0045 * smoothstep(0.12, 0.35, p.y) * mix(0.2, 1.0, smoothstep(-0.01, 0.03, dh));
  let tufts = 0.5 + 0.5 * wnoise(p * vec3f(40.0, 9.0, 22.0));
  let belly = (1.0 - smoothstep(0.47, 0.56, p.y)) * smoothstep(-0.22, -0.10, p.z) * (1.0 - smoothstep(0.18, 0.30, p.z)) * smoothstep(0.30, 0.40, p.y);
  let britches = smoothstep(-0.48, -0.56, p.z) * smoothstep(0.30, 0.40, p.y) * (1.0 - smoothstep(0.58, 0.64, p.y)) * smoothstep(0.02, 0.05, abs(p.x));
  return 0.9 * (d - amp * fur - 0.010 * tufts * max(belly, britches));
}

/** The smooth-union chain without fur: (distance, head distance or a bound on it). */
fn wolf_body(p: vec3f) -> vec2f {
  let m = mirror_x(p);
  var d = sd_torso(p);
  if (wbox(p, vec3f(-0.16, 0.52, 0.10), vec3f(0.16, 1.00, 0.64)) < d + 0.08) { d = smin(d, sd_neck(p), 0.08); }
  let bh = wbox(p, vec3f(-0.15, 0.71, 0.42), vec3f(0.15, 1.13, 0.89));
  var dh = max(bh, 0.03);
  if (bh < d + 0.05) { dh = sd_head(p); d = smin(d, dh, 0.05); }
  if (wbox(m, vec3f(-0.02, -0.03, 0.11), vec3f(0.16, 0.83, 0.43)) < d + 0.05) { d = smin(d, sd_foreleg(m), 0.05); }
  if (wbox(m, vec3f(-0.02, -0.03, -0.64), vec3f(0.17, 0.74, -0.14)) < d + 0.07) { d = smin(d, sd_hindleg(m), 0.07); }
  if (wbox(p, vec3f(-0.09, 0.22, -0.81), vec3f(0.09, 0.74, -0.50)) < d + 0.04) { d = smin(d, sd_tail(p), 0.04); }
  return vec2f(d, dh);
}

/** The body's normal without the fur's millimetre noise (wolf frame), for orienting brush strokes. */
fn wolf_smooth_n(q: vec3f) -> vec3f {
  let e = 0.008;
  let k = vec2f(1.0, -1.0);
  return normalize(k.xyy * wolf_body(q + k.xyy * e).x + k.yyx * wolf_body(q + k.yyx * e).x +
                   k.yxy * wolf_body(q + k.yxy * e).x + k.xxx * wolf_body(q + k.xxx * e).x);
}

fn wolf_hit(p: vec3f, sw: f32) -> Hit {
  let q = wolf_local(p);
  let bd = sd_box(q - vec3f(0.0, 0.52, 0.02), vec3f(0.27, 0.56, 0.90));
  if (bd > BOUND_SKIP * 0.3 + sw) { return Hit(bd, bd, MAT_WOLF); }
  var d = 0.0;
  if (EXACT_WOLF) { d = wolf_field(q); } else { d = wolf_fast(q, sw); }
  return Hit(d, d * 0.87, MAT_WOLF);      // the authoring tool measured a gradient up to 1.15
}

// ---- the scene ----------------------------------------------------------------------------------------

override DBG_OFF: u32 = 0u;          // profiling only: 1 trees, 2 wolf, 4 tower, 8 rocks left out

fn scene(p: vec3f, fp: f32, sw: f32, trees_on: bool) -> Hit {
  var h = Hit(INF, INF, MAT_STONE);
  if ((DBG_OFF & 4u) == 0u) { h = tower_hit(p, sw); }
  if ((DBG_OFF & 8u) == 0u) { h = hit_min(h, rock_hit(p, sw)); }
  if ((DBG_OFF & 2u) == 0u) { h = hit_min(h, wolf_hit(p, sw)); }
  if (trees_on && (DBG_OFF & 1u) == 0u) { h = hit_min(h, tree_hit(p, tree_index(p), fp, sw)); }
  return h;
}

fn ground_d(p: vec3f) -> f32 { return (p.y - terrain_h(p.x, p.z)) * T_COS; }

/** Objects and ground, for AO, thickness and curvature. */
fn scene_d(p: vec3f, sw: f32) -> f32 {
  return min(scene(p, 0.0, sw, true).d, ground_d(p));
}

/** The objects' gradient by tetrahedral differences (4 evaluations). */
fn scene_n(p: vec3f, fp: f32, e: f32) -> vec3f {
  let k = vec2f(1.0, -1.0);
  return normalize(k.xyy * scene(p + k.xyy * e, fp, 0.05, true).d + k.yyx * scene(p + k.yyx * e, fp, 0.05, true).d +
                   k.yxy * scene(p + k.yxy * e, fp, 0.05, true).d + k.xxx * scene(p + k.xxx * e, fp, 0.05, true).d);
}
