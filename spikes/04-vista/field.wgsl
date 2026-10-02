// The world field: one function of position (metres, content space) and a filtering footprint.
// Terrain heightfield + rock outcrops and crags (hash-placed) + authored ruins (CSG).
//
// Two interpretations, as a compiler would derive them:
//   world(p, fp)           a distance bound (Lipschitz ≤ 1 under the assumed facts below)
//   world_step(p, d, fp)   a safe step along the ray direction d (directional bounds: spike 02's
//                          heightfield trick, and cell exits for the hash-placed rocks)
//
// Assumed facts (D-077 @assume style), checked by sampling and by the reference comparison:
//   |∇h| ≤ T_G for the terrain; rock displacement slope ≤ R_LIP.

@group(0) @binding(4) var<storage, read> ruins: array<vec4f>;
@group(0) @binding(6) var<storage, read> rockbase: array<f32>;   // cooked scatter: base height per cell

override PARTS: u32 = 7u;              // P_TERRAIN | P_ROCKS | P_RUINS

const TAU: f32 = 6.2831853;

// ---- terrain ------------------------------------------------------------------------------------

const T_OCT: u32 = 15u;                // 2000 m down to 12 cm wavelength
const T_L0: f32 = 2000.0;
const T_RIDGED: u32 = 0u;              // ridged first octaves (none: the fold made the damping too steep)
const T_G: f32 = 3.0;                  // assumed slope bound
const T_K: f32 = 0.3162278;            // 1/sqrt(1 + T_G²)
const T_ROUGH: f32 = 0.08;             // amplitude floor per octave, as a fraction of wavelength
const T_YMIN: f32 = -100.0;            // the terrain stays within [T_YMIN, T_YMAX] (checked by sampling)
const T_YMAX: f32 = 2200.0;
const M2 = mat2x2f(0.8, 0.6, -0.6, 0.8);

fn mountain_mask(q: vec2f) -> f32 {
  let n = noise2(q * (1.0 / 5200.0) + vec2f(3.1, 7.7), 11u).x * 0.7
        + noise2(M2 * q * (1.0 / 2100.0) + vec2f(-1.3, 4.2), 12u).x * 0.3;
  return smoothstep(-0.45, 0.35, n);
}

/** Terrain amplitude in metres at mountain mask m. */
fn t_amp(m: f32) -> f32 { return 60.0 + 900.0 * m * m; }

/** Eroded fBm (derivative-damped value noise; ridged first octaves), footprint-filtered. Metres. */
fn terrain_h(q: vec2f, fp: f32) -> f32 {
  let m = mountain_mask(q);
  let amp = t_amp(m);
  var p = q * (1.0 / T_L0) + vec2f(0.37, 1.91);
  var a = 0.0;
  var b = 1.0;
  var d = vec2f(0.0);
  var wl = T_L0;
  for (var i = 0u; i < T_OCT; i++) {
    let w = octave_weight(fp, wl);
    if (w <= 0.0) { break; }
    var n = noise2(p, 101u + i);
    if (i < T_RIDGED) {
      // ridged, with a smooth fold so the field (and the damping below) stays continuous
      let r = sqrt(n.x * n.x + 0.01);
      n = vec3f(1.0 - 2.0 * r, -2.0 * (n.x / r) * n.yz) * 0.5;
    }
    d += w * n.yz;
    let oct = max(b * amp, T_ROUGH * wl);      // metres
    a += w * oct * n.x / (1.0 + dot(d, d));
    b *= select(0.42, 0.5, i < 3u);
    p = M2 * p * 2.0;
    wl *= 0.5;
  }
  // Valley floors: a smooth max with a gently rolling floor flattens the lowest ground.
  let floor_h = 25.0 + 12.0 * noise2(q * (1.0 / 900.0), 13u).x;
  let raw = 40.0 + 420.0 * m + a;
  let k = 40.0;
  let hh = clamp(0.5 + 0.5 * (raw - floor_h) / k, 0.0, 1.0);
  return mix(floor_h, raw, hh) + k * hh * (1.0 - hh);
}

// ---- rocks: hash-placed, each entirely inside its own square cell -----------------------------------

const R_LIP: f32 = 0.8;                // assumed displacement slope
const R_K: f32 = 0.5555556;            // 1 / (1 + R_LIP)

struct RockKind {
  cell: f32,     // cell size (m)
  rmin: f32,
  rmax: f32,
  pad: f32,      // centres lie in [pad, cell − pad]; the rock's extent is ≤ pad − gap
  gap: f32,
  prob: f32,
  amp: f32,      // displacement amplitude, as a fraction of size
  band: f32,     // rocks only exist where y − h(x, z) ≤ band
  seed: u32,
  crag: u32,
}

fn boulders() -> RockKind { return RockKind(48.0, 1.2, 5.0, 11.0, 2.0, 0.42, 0.16, 40.0, 501u, 0u); }
fn crags() -> RockKind { return RockKind(240.0, 16.0, 40.0, 96.0, 4.0, 0.8, 0.12, 260.0, 601u, 1u); }

struct Rock { cxz: vec2f, ext: vec3f, rot: vec4f, size: f32, yoff: f32, ok: bool }

/** A cell's rock, without its base height (which costs a terrain evaluation). */
fn rock_params(ci: vec2i, k: RockKind) -> Rock {
  var r: Rock;
  let h0 = hash2i(ci, k.seed);
  let h1 = hash_u(h0 + 1u);
  let h2 = hash_u(h0 + 2u);
  let h3 = hash_u(h0 + 3u);
  let u = vec4f(f32(h0 & 0xFFFFu), f32(h0 >> 16u), f32(h1 & 0xFFFFu), f32(h1 >> 16u)) / 65535.0;
  let v = vec4f(f32(h2 & 0xFFFFu), f32(h2 >> 16u), f32(h3 & 0xFFFFu), f32(h3 >> 16u)) / 65535.0;
  let lo = vec2f(ci) * k.cell;
  r.cxz = lo + k.pad + u.yz * (k.cell - 2.0 * k.pad);
  var prob = k.prob;
  if (k.crag == 1u) { prob *= smoothstep(0.45, 0.85, mountain_mask(r.cxz)); }
  r.ok = u.x < prob;
  let s = mix(k.rmin, k.rmax, u.w * u.w);
  r.size = s;
  r.ext = s * select(vec3f(1.0, 0.45 + 0.3 * v.x, 0.6 + 0.35 * v.y), vec3f(1.0, 0.35 + 0.2 * v.x, 0.3 + 0.15 * v.y), k.crag == 1u);
  let yaw = v.z * TAU;
  let tilt = (v.w - 0.5) * select(0.5, 0.6, k.crag == 1u);
  r.rot = vec4f(cos(yaw), sin(yaw), cos(tilt), sin(tilt));
  r.yoff = r.ext.y * select(0.25, -0.5, k.crag == 1u);
  return r;
}

/** Rocks are scattered over the 8×8 km square only; the scatter (each cell's base height) is cooked
 *  once into `rockbase` by `place_rocks`, as an engine would place instances at load. */
const R_REGION: f32 = 4096.0;

fn rock_slot(ci: vec2i, k: RockKind) -> i32 {
  let n = i32(ceil(R_REGION / k.cell));
  let q = ci + vec2i(n);
  if (any(q < vec2i(0)) || any(q >= vec2i(2 * n))) { return -1; }
  return select(0, 2 * 86 * 2 * 86, k.crag == 1u) + q.y * 2 * n + q.x;
}

/** The base: the lowest ground under the rock's footprint, so it doesn't float on slopes. A fixed
 *  filter (not the footprint), so the rock doesn't move between levels of detail. */
fn rock_base(r: Rock) -> f32 {
  var lo = terrain_h(r.cxz, r.size * 0.25);
  for (var i = 0u; i < 8u; i++) {
    let a = f32(i) * (TAU / 8.0);
    lo = min(lo, terrain_h(r.cxz + 0.8 * r.size * vec2f(cos(a), sin(a)), r.size * 0.25));
  }
  return lo;
}

fn rock_center(r: Rock, slot: i32) -> vec3f {
  return vec3f(r.cxz.x, rockbase[slot] + r.yoff, r.cxz.y);
}

fn sd_round_box(p: vec3f, b: vec3f, r: f32) -> f32 {
  let q = abs(p) - b + r;
  return length(max(q, vec3f(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0) - r;
}

fn fbm3(p: vec3f, fp: f32, wl0: f32, seed: u32) -> f32 {
  var s = 0.0;
  var a = 0.5;
  var wl = wl0;
  var q = p / wl0;
  for (var o = 0u; o < 7u; o++) {
    let w = octave_weight(fp, wl);
    if (w <= 0.0 || wl < 0.1) { break; }
    s += w * a * noise3(q, seed + o);
    a *= 0.5;
    wl *= 0.5;
    q = q * 2.0 + vec3f(5.3, 1.7, 9.1);
  }
  return s;   // |s| < 1
}

/** Distance bound to the rock (local displacement included when near). */
fn rock_sd(p: vec3f, c: vec3f, r: Rock, k: RockKind, fp: f32) -> f32 {
  var q = p - c;
  q = vec3f(r.rot.x * q.x - r.rot.y * q.z, q.y, r.rot.y * q.x + r.rot.x * q.z);
  q = vec3f(q.x, r.rot.z * q.y - r.rot.w * q.z, r.rot.w * q.y + r.rot.z * q.z);
  let rr = select(0.35, 0.18, k.crag == 1u) * min(r.ext.x, min(r.ext.y, r.ext.z));
  let b = sd_round_box(q, r.ext - rr, rr);
  let amp = k.amp * r.size;
  if (b > amp * 1.2) { return b - amp; }
  var disp = fbm3(q, fp, 0.6 * r.size, k.seed + 17u);
  if (k.crag == 1u) {
    // ledges in the crag's own (tilted) frame, broken up by noise
    disp += 0.25 * sin(q.y * (TAU / 3.1) + 3.0 * noise3(q * 0.09, k.seed + 31u)) * octave_weight(fp, 3.1);
  }
  return (b + amp * disp) * R_K;
}

/** Rocks of one kind near p. x: distance bound (field); y: safe step along d (d may be zero).
 *  Rocks live inside their own cells, so evaluating the 2×2 block of cells nearest to p is exact
 *  for every rock within half a cell; anything outside the block is at least (distance to the
 *  block's border + gap) away. */
fn rocks_kind(p: vec3f, d: vec3f, fp: f32, k: RockKind, gap_above: f32) -> vec2f {
  let c = floor(p.xz / k.cell - 0.5);
  let ci = vec2i(c);
  let lo = c * k.cell;
  let sz = 2.0 * k.cell;
  let f = p.xz - lo;
  let border = min(min(f.x, sz - f.x), min(f.y, sz - f.y));
  let dxz = d.xz;
  let tx = select(select((lo.x - p.x) / dxz.x, (lo.x + sz - p.x) / dxz.x, dxz.x > 0.0), INF, abs(dxz.x) < 1e-7);
  let tz = select(select((lo.y - p.z) / dxz.y, (lo.y + sz - p.z) / dxz.y, dxz.y > 0.0), INF, abs(dxz.y) < 1e-7);
  var dist = border + k.gap;
  var step = min(tx, tz) + k.gap;
  // Above the band, no rock of this kind can be near.
  if (gap_above > k.band) {
    dist = max(dist, (gap_above - k.band) * T_K);
    let rate = T_G * length(d.xz) - d.y;
    if (rate <= 0.0) { step = INF; } else { step = max(step, (gap_above - k.band) / rate); }
    return vec2f(dist, step);
  }
  for (var j = 0; j < 4; j++) {
    let cj = ci + vec2i(j & 1, j >> 1);
    let slot = rock_slot(cj, k);
    if (slot < 0) { continue; }
    let r = rock_params(cj, k);
    if (!r.ok) { continue; }
    let bound_r = length(r.ext) + k.amp * r.size;
    let cc = rock_center(r, slot);
    let dc = length(p - cc) - bound_r;
    var rd = dc;
    if (dc < 1.0) { rd = rock_sd(p, cc, r, k, fp); }
    dist = min(dist, rd);
    step = min(step, rd);
  }
  return vec2f(dist, step);
}

// ---- ruins (CSG) --------------------------------------------------------------------------------

fn sd_box(p: vec3f, b: vec3f) -> f32 {
  let q = abs(p) - b;
  return length(max(q, vec3f(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

/** Polar repetition: rotates p.xz into the first of n sectors, centred on +x. */
fn polar(p: vec3f, n: f32) -> vec3f {
  let a = atan2(p.z, p.x);
  let s = TAU / n;
  let a2 = (fract(a / s + 0.5) - 0.5) * s;
  let r = length(p.xz);
  return vec3f(r * cos(a2), p.y, r * sin(a2));
}

/** A ragged break: removes material above a tilted, noisy plane. */
fn broken(q: vec3f, d: f32, nrm: vec3f, cut: f32, seed: u32) -> f32 {
  let plane = (dot(q, nrm) - cut + 1.4 * noise3(q * 0.3, seed)) / 2.4;
  return max(d, -plane);
}

/** Round tower, base at y = 0: radius R, height H, wall w, crenellations, windows, door, top floor. */
fn tower(q: vec3f, R: f32, H: f32, w: f32, brk: f32, seed: u32) -> f32 {
  let r = length(q.xz);
  let shell = max(abs(r - (R - 0.5 * w)) - 0.5 * w, max(q.y - H, -q.y - 3.0));
  // corbelled parapet, slightly proud of the wall
  let par = max(abs(r - (R - 0.3 * w)) - 0.5 * w - 0.15, abs(q.y - (H + 0.1)) - 0.5);
  var d = min(shell, par);
  // merlons
  let pq = polar(q, 12.0);
  let merlon = sd_box(pq - vec3f(R - 0.3 * w, H + 0.95, 0.0), vec3f(0.5 * w + 0.1, 0.45, 0.62));
  d = min(d, merlon);
  // top floor
  d = min(d, max(r - (R - 0.5 * w), abs(q.y - (H - 0.35)) - 0.3));
  // a string course every so often
  let sc = max(abs(r - R) - 0.12, abs(fract(q.y / 7.0) * 7.0 - 3.5) - 3.3);
  d = min(d, max(sc, q.y - H + 1.0));
  // windows (slits), four sectors, at three heights
  let wq = polar(q, 4.0);
  let wy = clamp(round((q.y - 6.0) / 6.0), 0.0, 2.0) * 6.0 + 6.0;
  let win = sd_box(wq - vec3f(R - 0.5 * w, wy, 0.0), vec3f(w, 0.9, 0.22));
  d = max(d, -win);
  // door
  let door = sd_box(q - vec3f(R - 0.5 * w, 1.2, 0.0), vec3f(w, 1.3, 0.65));
  d = max(d, -door);
  if (brk > 0.0) {
    d = broken(q, d, normalize(vec3f(0.55, 1.0, 0.25)), H * (1.0 - 0.18 * brk), seed);
  }
  return d;
}

/** 2D arch: a box of half-size (hw, hh) with a half-disc of radius hw on top. */
fn arch2(x: f32, y: f32, hw: f32, hh: f32) -> f32 {
  let q = abs(vec2f(x, y)) - vec2f(hw, hh);
  let bx = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0);
  return min(bx, length(vec2f(x, y - hh)) - hw);
}

/** Crenellation notches along a coordinate s: negative inside a notch. Lipschitz ≤ 1 in s. */
fn notch(s: f32) -> f32 { return abs(fract(s / 2.4) * 2.4 - 1.2) - 0.55; }

/** Rectangular keep: outer half-extents (a, b), height H, wall w, arched windows, crenellations. */
fn keep(q: vec3f, a: f32, b: f32, H: f32, w: f32, brk: f32, seed: u32) -> f32 {
  let outer = sd_box(q - vec3f(0.0, 0.5 * H - 1.5, 0.0), vec3f(a, 0.5 * H + 1.5, b));
  let inner = sd_box(q - vec3f(0.0, 0.5 * H + 0.5, 0.0), vec3f(a - w, 0.5 * H + 2.0, b - w));
  var d = max(outer, -inner);
  // merlons along the wall tops
  let ring = max(sd_box(q - vec3f(0.0, H + 0.6, 0.0), vec3f(a, 0.6, b)), -sd_box(q - vec3f(0.0, H + 0.6, 0.0), vec3f(a - w, 1.0, b - w)));
  let n = select(notch(q.z), notch(q.x), abs(abs(q.z) - b) < abs(abs(q.x) - a));
  d = min(d, max(ring, -n));
  // an upper floor, broken through
  let fl = max(sd_box(q - vec3f(0.0, 7.0, 0.0), vec3f(a - w, 0.25, b - w)), -(length(q.xz - vec2f(0.3 * a, -0.2 * b)) - 0.45 * min(a, b)));
  d = min(d, fl);
  // arched windows, two storeys, repeated along each wall
  let nx = floor((a - 2.5) / 4.5);
  let nz = floor((b - 2.5) / 4.5);
  let rx = q.x - clamp(round(q.x / 4.5), -nx, nx) * 4.5;
  let rz = q.z - clamp(round(q.z / 4.5), -nz, nz) * 4.5;
  let wy = select(3.6, 9.6, q.y > 7.0);
  let wa = max(arch2(rx, q.y - wy, 0.6, 1.0), abs(abs(q.z) - (b - 0.5 * w)) - (0.5 * w + 0.4));
  let wb = max(arch2(rz, q.y - wy, 0.6, 1.0), abs(abs(q.x) - (a - 0.5 * w)) - (0.5 * w + 0.4));
  d = max(d, -min(wa, wb));
  // gate in the -z wall
  let gate = max(arch2(q.x, q.y - 1.0, 1.4, 1.6), abs(q.z + b - 0.5 * w) - (0.5 * w + 0.4));
  d = max(d, -gate);
  if (brk > 0.0) {
    d = broken(q, d, normalize(vec3f(-0.6, 1.0, 0.45)), H * (1.0 - 0.25 * brk), seed);
  }
  return d;
}

/** A curtain wall along local x: half-length a, height H, thickness w. */
fn wall(q: vec3f, a: f32, H: f32, w: f32, brk: f32, seed: u32) -> f32 {
  var d = sd_box(q - vec3f(0.0, 0.5 * H - 1.5, 0.0), vec3f(a, 0.5 * H + 1.5, 0.5 * w));
  let mer = sd_box(q - vec3f(0.0, H + 0.6, 0.0), vec3f(a, 0.6, 0.5 * w));
  d = min(d, max(mer, -notch(q.x)));
  if (brk > 0.0) {
    d = broken(q, d, normalize(vec3f(0.35, 1.0, 0.1)), H * (1.0 - 0.3 * brk), seed);
  }
  return d;
}

/** All ruins: x = distance bound. Each structure is skipped via its bounding cylinder. */
fn ruins_d(p: vec3f) -> f32 {
  var best = INF;
  let n = u32(ruins[0].x);
  for (var i = 0u; i < n; i++) {
    let a = ruins[1u + 3u * i];
    let b = ruins[2u + 3u * i];
    let c = ruins[3u + 3u * i];
    var q = p - a.xyz;
    let bd = max(length(q.xz) - c.w, max(-q.y - 4.0, q.y - c.x - 4.0));
    if (bd > 0.5) { best = min(best, bd); continue; }
    q = vec3f(b.x * q.x - b.y * q.z, q.y, b.y * q.x + b.x * q.z);
    let kind = u32(a.w);
    var d = INF;
    if (kind == 0u) { d = tower(q, b.z, c.x, c.y, c.z, 900u + i); }
    else if (kind == 1u) { d = keep(q, b.z, b.w, c.x, c.y, c.z, 900u + i); }
    else { d = wall(q, b.z, c.x, c.y, c.z, 900u + i); }
    best = min(best, d);
  }
  return best;
}

// ---- the world ------------------------------------------------------------------------------------

struct Sample {
  comp: u32,    // the closest component: 0 terrain, 1 rocks, 2 ruins
  d: f32,       // distance bound (the field)
  s: f32,       // safe step along the ray
  ruin: f32,    // the ruins' own distance (material)
  gap: f32,     // y − h(x, z)
}

/** The world field and its directional step. `d` may be zero (no ray): s is then the bound. */
fn world_sample(p: vec3f, d: vec3f, fp: f32) -> Sample {
  var o: Sample;
  o.comp = 0u;
  o.d = INF;
  o.s = INF;
  o.ruin = INF;
  o.gap = INF;
  if ((PARTS & P_TERRAIN) != 0u) {
    let h = terrain_h(p.xz, fp);
    let g = p.y - h;
    o.gap = g;
    o.d = g * T_K;
    let rate = T_G * length(d.xz) - d.y;
    if (g < 0.0) { o.s = o.d; }
    else if (rate <= 0.0) { o.s = INF; }
    else { o.s = g / rate; }
    if ((PARTS & P_ROCKS) != 0u) {
      let a = rocks_kind(p, d, fp, boulders(), g);
      let b = rocks_kind(p, d, fp, crags(), g);
      if (min(a.x, b.x) < o.d) { o.comp = 1u; }
      o.d = min(o.d, min(a.x, b.x));
      o.s = min(o.s, min(a.y, b.y));
    }
  }
  if ((PARTS & P_RUINS) != 0u) {
    let r = ruins_d(p);
    o.ruin = r;
    if (r < o.d) { o.comp = 2u; }
    o.d = min(o.d, r);
    o.s = min(o.s, r);
  }
  return o;
}

fn world(p: vec3f, fp: f32) -> f32 {
  // With no ray direction, the "step" terms degrade to perpendicular bounds: use the field.
  return world_sample(p, vec3f(0.0, 0.0, 0.0), fp).d;
}

fn rocks_only(p: vec3f, fp: f32) -> f32 {
  let g = p.y - terrain_h(p.xz, fp);
  return min(rocks_kind(p, vec3f(0.0), fp, boulders(), g).x, rocks_kind(p, vec3f(0.0), fp, crags(), g).x);
}

/** Normal from the field, of the closest component only (a min's other branches and the rocks'
 *  cheap bounds would otherwise leak into the finite differences): terrain from its height
 *  gradient, rocks and ruins by tetrahedral differences. Offsets are footprint-sized. */
fn world_normal(p: vec3f, fp: f32) -> vec3f {
  let e = max(fp, 0.002);
  let s = world_sample(p, vec3f(0.0), fp);
  if (s.comp == 0u) {
    let hx = terrain_h(p.xz + vec2f(e, 0.0), fp) - terrain_h(p.xz - vec2f(e, 0.0), fp);
    let hz = terrain_h(p.xz + vec2f(0.0, e), fp) - terrain_h(p.xz - vec2f(0.0, e), fp);
    return normalize(vec3f(-hx, 2.0 * e, -hz));
  }
  let k0 = vec3f(1.0, -1.0, -1.0);
  let k1 = vec3f(-1.0, -1.0, 1.0);
  let k2 = vec3f(-1.0, 1.0, -1.0);
  let k3 = vec3f(1.0, 1.0, 1.0);
  if (s.comp == 2u) {
    return normalize(k0 * ruins_d(p + k0 * e) + k1 * ruins_d(p + k1 * e) + k2 * ruins_d(p + k2 * e) + k3 * ruins_d(p + k3 * e));
  }
  return normalize(k0 * rocks_only(p + k0 * e, fp) + k1 * rocks_only(p + k1 * e, fp)
                 + k2 * rocks_only(p + k2 * e, fp) + k3 * rocks_only(p + k3 * e, fp));
}
