// Shared by trace.wgsl and light.wgsl (appended to common.wgsl): the leaf tiles, the volumetric
// textures, the calibration, the grid walk, and the three foliage levels' visibility.

@group(0) @binding(1) var<storage, read> leaves: array<u32>;
@group(0) @binding(2) var tex_b: texture_3d<f32>;
@group(0) @binding(3) var tex_c: texture_3d<f32>;
@group(0) @binding(4) var samp: sampler;
@group(0) @binding(9) var<storage, read> tmap: array<vec4u>;

override REF: bool = false;           // the brute-force reference: explicit leaves everywhere, raised caps
override MAP: bool = true;            // per-cell tree attributes from the cooked table (false: evaluate inline)
override STATS: bool = false;
override VIEW: u32 = 0u;              // 0 shaded, 1 primary-work heat map, 2 shadow-work heat map

const TEX_CELLS: f32 = 16.0;          // the tile period in leaf cells
const TEXELS_PER_CELL: f32 = 8.0;

fn cap_slabs() -> u32 { return select(160u, 1600u, REF); }
fn cap_leaf_cells() -> u32 { return select(96u, 1024u, REF); }
fn cap_vol() -> u32 { return 48u; }

struct Counters {
  steps: u32, slabs: u32, cells: u32, trees: u32, l0: u32, l1: u32, l2: u32, l0v: u32, leaf_cells: u32, leaf_tests: u32,
  vol: u32, gslabs: u32, gcells: u32, blades: u32, fcells: u32, fronds: u32, branch: u32, trunk: u32,
  cap_t: u32, cap_s: u32, cap_l: u32, cap_v: u32, cap_g: u32, cap_f: u32,
}

// ---- trees -----------------------------------------------------------------------------------------------------

struct TreeLite { exists: bool, kind: u32, pos: vec2f, base: f32, rb: f32, top: f32 }

/** What the grid walk needs before the tree's shape: existence, position, a horizontal bound and the
 *  height range. MAP reads them from the cooked table (two vec4u per cell); without it, existence and
 *  position come from the hash and the bounds are the slot's conservative ones. */
fn tree_lite(ci: vec2i, slot: u32) -> TreeLite {
  var l: TreeLite;
  l.exists = false;
  if (F.iso.z == 1 && (any(ci != F.iso.xy) || slot != 0u)) { return l; }
  if (MAP) {
    let m = ci + vec2i(MAP_HALF);
    if (any(m < vec2i(0)) || any(m >= vec2i(2 * MAP_HALF))) { return l; }
    let e = tmap[(u32(m.y) * u32(2 * MAP_HALF) + u32(m.x)) * 2u + slot];
    l.exists = (e.x & 1u) == 1u;
    l.kind = (e.x >> 1u) & 1u;
    l.base = bitcast<f32>(e.y);
    let bt = unpack2x16float(e.z);
    l.rb = bt.x;
    l.top = l.base + bt.y;
    l.pos = (vec2f(ci) + 0.5) * TREE_C + (unpack2x16unorm(e.w) - 0.5) * 2.0 * TREE_J;
  } else {
    let h = tree_hash(ci, slot);
    l.pos = tree_pos(ci, h);
    l.exists = u01(h.x) < tree_occ(slot) * forest_mask(l.pos.x, l.pos.y);
    l.rb = slot_bound(slot);
    l.base = -INF;
    l.top = INF;
  }
  return l;
}

struct Tree {
  exists: bool,
  slot: u32,
  kind: u32,          // 0 broadleaf, 1 conifer
  id: u32,
  pos: vec2f,         // trunk xz
  base: f32,          // ground height at the trunk (−0.2 m)
  size: f32,
  rh: f32,            // broadleaf: horizontal radius; conifer: base radius
  rv: f32,            // broadleaf: vertical radius; conifer: crown height
  height: f32,
  cy: f32,            // broadleaf: crown centre above the base; conifer: crown base above the base
  r0: f32,            // trunk radius at the base
  tint: f32,
  cs: vec2f,          // cos, sin of the crown frame's yaw
  pcs: vec2f,         // cos, sin of ph.x (the clump ring's phase)
  ph: vec3f,          // envelope phases
  off: vec3f,         // leaf tile offset (cells)
  sway: vec2f,        // the crown's wind offset now (xz)
}

fn tree_full(l: TreeLite, ci: vec2i, slot: u32) -> Tree {
  var t: Tree;
  t.exists = l.exists;
  t.slot = slot;
  t.pos = l.pos;
  let h = tree_hash(ci, slot);
  t.id = h.x;
  let h2 = pcg3(h.yzx);
  let h3 = pcg3(vec3u(h2.z, h2.x, h2.y ^ 0x68E31DA4u));
  let h4 = pcg3(vec3u(h3.y, h3.z, h3.x + 77u));
  if (MAP) { t.kind = l.kind; t.base = l.base; }
  else {
    t.kind = select(0u, 1u, u01(h2.x) < conifer_share(t.pos.x, t.pos.y));
    t.base = terrain_h(t.pos.x, t.pos.y) - 0.2;
  }
  t.size = u01(h2.y);
  let u = u01(h2.z);
  let v = u01(h3.x);
  let s = t.size;
  if (slot == 0u) {
    if (t.kind == 0u) { t.rh = 2.8 + 1.8 * u; t.rv = t.rh * (0.9 + 0.45 * v); t.height = 14.0 + 10.0 * s; t.cy = t.height - t.rv; t.r0 = 0.18 + 0.16 * s; }
    else { t.rh = 2.5 + 1.0 * u; t.height = 15.0 + 11.0 * s; t.cy = 1.0 + 2.0 * v; t.rv = t.height - t.cy; t.r0 = 0.16 + 0.14 * s; }
  } else {
    if (t.kind == 0u) { t.rh = 1.2 + 1.3 * u; t.rv = t.rh * (0.85 + 0.4 * v); t.height = 3.5 + 5.0 * s; t.cy = t.height - t.rv; t.r0 = 0.05 + 0.06 * s; }
    else { t.rh = 0.9 + 1.0 * u; t.height = 3.0 + 5.0 * s; t.cy = 0.3 + 0.5 * v; t.rv = t.height - t.cy; t.r0 = 0.05 + 0.05 * s; }
  }
  t.tint = u01(h3.y);
  let yaw = u01(h3.z) * 6.2831853;
  t.cs = vec2f(cos(yaw), sin(yaw));
  t.off = vec3f(f32((h3.x >> 3u) & 15u), f32((h3.y >> 11u) & 15u), f32((h3.z >> 19u) & 15u));
  t.ph = u01v(h4) * 6.2831853;
  t.pcs = vec2f(cos(t.ph.x), sin(t.ph.x));
  // Wind: the crown sways as a whole (a bounded rigid offset, so every bound just grows by SWAY_MAX).
  let w = F.cr.w;
  let tm = F.cf.w;
  let a = w * (0.10 + 0.10 * s) * (sin(0.9 * tm + t.ph.x) + 0.4 * sin(2.3 * tm + 2.0 * t.ph.y));
  let b = w * 0.03 * sin(1.3 * tm + t.ph.z);
  t.sway = WIND_DIR * a + vec2f(-WIND_DIR.y, WIND_DIR.x) * b;
  return t;
}

/** The horizontal radius that bounds everything the tree draws (trunk, branches, crown, sway). */
fn tree_bound_r(t: Tree) -> f32 { return select(1.3 * t.rh, t.rh, t.kind == 0u) + SWAY_MAX; }

/** Crown frame origin: the crown centre (broadleaf) or crown base centre (conifer), swayed. */
fn crown_origin(t: Tree) -> vec3f { return vec3f(t.pos.x + t.sway.x, t.base + t.cy, t.pos.y + t.sway.y); }
fn to_crown(t: Tree, rel: vec3f) -> vec3f {   // world offset → crown frame (rotated by yaw)
  return vec3f(t.cs.x * rel.x + t.cs.y * rel.z, rel.y, -t.cs.y * rel.x + t.cs.x * rel.z);
}
fn from_crown(t: Tree, l: vec3f) -> vec3f {   // crown frame direction → world
  return vec3f(t.cs.x * l.x - t.cs.y * l.z, l.y, t.cs.y * l.x + t.cs.x * l.z);
}
/** The leaf grid is tilted per tree (up to ±26° about two axes), so its cell planes don't line up
 *  between trees or with the view. Crown frame → leaf grid frame. */
fn leaf_rot(t: Tree) -> mat3x3f {
  let h5 = pcg3(vec3u(t.id, t.id ^ 0x85EBCA6Bu, 0x27d4eb2fu));
  let a = (u01(h5.x) - 0.5) * 0.9;
  let b = (u01(h5.y) - 0.5) * 0.9;
  let ca = cos(a); let sa = sin(a); let cb = cos(b); let sb = sin(b);
  // Rx(a) · Rz(b), columns
  return mat3x3f(vec3f(cb, ca * sb, sa * sb), vec3f(-sb, ca * cb, sa * cb), vec3f(0.0, -sa, ca));
}

struct Env { ef: f32, prof: f32, ao: f32, n: vec3f }

fn clump(q: vec3f, dir: vec3f, s: f32) -> vec4f {
  let c = q - dir * (1.0 - s);
  let l = length(c);
  return vec4f(s - l, c / max(l, 1e-6));
}

/** Crown envelope in the crown frame. ef > 0 inside (a normalized depth), prof the leaf density
 *  profile, ao an occlusion estimate, n the envelope's outward normal (crown frame). Broadleaf: a
 *  union of seven foliage clumps inside the (rh, rv, rh) ellipsoid. Conifer: a cone of whorl tiers. */
fn crown_env(t: Tree, l: vec3f) -> Env {
  var e: Env;
  if (t.kind == 0u) {
    let R = vec3f(t.rh, t.rv, t.rh);
    let q = l / R;
    var best = clump(q, vec3f(0.0, 1.0, 0.0), 0.5 + 0.1 * fract(t.ph.y * 1.618));
    var cs = t.pcs;
    for (var k = 0u; k < 5u; k++) {
      let c = clump(q, vec3f(cs.x * 0.92, 0.39, cs.y * 0.92), 0.42 + 0.16 * fract(t.ph.y * f32(k + 2u) * 1.618));
      if (c.x > best.x) { best = c; }
      cs = vec2f(cs.x * 0.30901699 - cs.y * 0.95105652, cs.x * 0.95105652 + cs.y * 0.30901699);
    }
    let cb = clump(q, vec3f(0.0, -0.8, 0.0), 0.45 + 0.1 * fract(t.ph.z * 1.618));
    if (cb.x > best.x) { best = cb; }
    e.ef = best.x;
    e.prof = smoothstep(0.0, 0.1, e.ef) * (1.0 - 0.3 * smoothstep(0.22, 0.45, e.ef));
    e.n = normalize(best.yzw / R);
    e.ao = mix(1.0, 0.3, smoothstep(0.0, 0.35, e.ef)) * mix(0.5, 1.0, smoothstep(0.2, 0.95, length(q)));
    return e;
  }
  let hc = t.rv;
  let f = l.y / hc;
  let u = (hc - l.y) / 1.3;
  let tier = floor(u);
  let ft = u - tier;
  let saw = select(ft, (1.0 - ft) / 0.15 * 0.85, ft > 0.85);
  let tv = 0.8 + 0.35 * fract(tier * 0.618 + t.ph.x);
  let r = length(l.xz);
  let rad = l.xz / max(r, 1e-4);
  // Each whorl is lopsided: a three-lobed modulation around the trunk, turned per tier.
  let ta = tier * 2.4 + t.ph.y;
  let cth = dot(rad, vec2f(cos(ta), sin(ta)));
  let lobe = 1.0 + 0.12 * (4.0 * cth * cth * cth - 3.0 * cth);
  let ra = (1.0 - f) * t.rh * (0.78 + 0.22 * saw) * tv * lobe;
  e.ef = min((ra - r) / t.rh, min(l.y, hc - l.y) / t.rh);
  e.prof = smoothstep(0.0, 0.08, e.ef) * (1.0 - 0.35 * smoothstep(0.2, 0.45, e.ef)) * (0.7 + 0.3 * saw);
  // Shading normal: outward from the trunk, tilted up by the cone's slope plus the droop of the
  // branch tips (the tier sawtooth's own slope flips sign every whorl and made banded 'ribbons').
  e.n = normalize(vec3f(rad.x, t.rh / hc + 0.35, rad.y));
  e.ao = mix(1.0, 0.35, smoothstep(0.0, 0.3, e.ef)) * mix(0.6, 1.0, saw);
  return e;
}

/** Metres per unit of ef: ef changes by at most 1 per this many metres, so a point with ef < 0 is at
 *  least −ef × env_lip(t) from the envelope. Broadleaf: max of 1-Lipschitz clump terms in coordinates
 *  scaled by (rh, rv, rh). Conifer: the cone's slope plus the whorl sawtooth's (bounded by 2/rh). */
fn env_lip(t: Tree) -> f32 { return select(0.5 * t.rh, min(t.rh, t.rv), t.kind == 0u); }

/** Where the ray first comes within `margin` metres of the crown envelope, by sphere tracing ef (a
 *  scaled distance bound): the leaf walk and the volume march start there instead of at the bound,
 *  skipping the empty space between the bounding ellipsoid or cone and the clumps. tb if never. */
fn env_entry(t: Tree, o: vec3f, d: vec3f, ta: f32, tb: f32, margin: f32) -> f32 {
  let org = crown_origin(t);
  let lo = to_crown(t, o - org);
  let ld = to_crown(t, d);
  let lip = env_lip(t);
  var tt = ta;
  for (var i = 0u; i < 10u; i++) {
    let gap = -crown_env(t, lo + ld * tt).ef * lip - margin;
    if (gap <= 0.0) { return tt; }
    tt += gap;
    if (tt >= tb) { return tb; }
  }
  return tt;
}

fn foliage_albedo(t: Tree, tint: f32) -> vec3f {
  var base: vec3f;
  if (t.kind == 0u) {
    base = mix(vec3f(0.042, 0.082, 0.020), vec3f(0.085, 0.112, 0.026), t.tint);
  } else {
    base = mix(vec3f(0.018, 0.040, 0.022), vec3f(0.028, 0.050, 0.020), t.tint);
  }
  return base * (0.7 + 0.6 * tint);
}

/** Broadleaf branch k (0–3): from the trunk inside the crown out toward a clump. */
fn branch(t: Tree, k: u32) -> array<vec4f, 2> {
  let c = crown_origin(t);
  let fk = f32(k);
  let ang = fk * 1.5708 + t.ph.x;
  let h = c.y - t.rv * (0.55 - 0.12 * fk);
  let a = vec3f(t.pos.x, h, t.pos.y);
  let dir = from_crown(t, vec3f(cos(ang), 0.0, sin(ang)));
  let b = vec3f(c.x + dir.x * t.rh * 0.6, c.y + t.rv * (0.1 + 0.08 * fk), c.z + dir.z * t.rh * 0.6);
  return array<vec4f, 2>(vec4f(a, t.r0 * (0.42 - 0.04 * fk)), vec4f(b, 0.0));
}

// ---- calibration and texture -----------------------------------------------------------------------------------

fn cval(kind: u32, m: u32, b: u32) -> f32 { let i = kind * 24u + m * 3u + b; return F.calib[i >> 2u][i & 3u]; }

/** The density factor for this species, mip and ray elevation (bands centred at |d.y| = 1/6, 1/2, 5/6). */
fn calib(kind: u32, mip: f32, ady: f32) -> f32 {
  let bp = clamp(ady * 3.0 - 0.5, 0.0, 2.0);
  let b0 = u32(bp); let b1 = min(b0 + 1u, 2u); let bf = bp - f32(b0);
  let m = clamp(mip, 0.0, 7.0);
  let m0 = u32(m); let m1 = min(m0 + 1u, 7u); let mf = m - f32(m0);
  return mix(mix(cval(kind, m0, b0), cval(kind, m0, b1), bf), mix(cval(kind, m1, b0), cval(kind, m1, b1), bf), mf);
}

fn tex_at(kind: u32, leafp: vec3f, mip: f32) -> vec4f {
  let uvw = leafp / TEX_CELLS;
  if (kind == 0u) { return textureSampleLevel(tex_b, samp, uvw, mip); }
  return textureSampleLevel(tex_c, samp, uvw, mip);
}

/** The mip and step for a volume march over `chord` metres with footprint `fp` and at most `steps` steps:
 *  steps are two texels of the chosen mip (the regime the calibration used). */
fn vol_setup(kind: u32, chord: f32, fp: f32, steps: f32) -> vec2f {
  let texel0 = leaf_cell_m(kind) / TEXELS_PER_CELL;
  let mip_fp = log2(max(fp / texel0, 1.0));
  let mip_dt = log2(max(chord / steps / (2.0 * texel0), 1.0));
  let mip = clamp(max(mip_fp, mip_dt), 0.0, 7.0);
  return vec2f(mip, 2.0 * texel0 * exp2(mip));
}

// ---- level choice --------------------------------------------------------------------------------------------------

/** 0 explicit leaves, 1 volumetric texture, 2 density volume. Chosen per crown per pixel by the footprint
 *  where the ray enters the crown (against k0 × the leaf cell, and k1 metres); across a boundary the choice is random with a ramped probability. */
fn choose_level(t: Tree, fp: f32, xi: f32) -> u32 {
  if (REF) { return 0u; }
  if (F.lod.w >= 0.0) { return u32(F.lod.w); }
  let w01 = clamp(log2(fp / (F.lod.x * leaf_cell_m(t.kind))), 0.0, 1.0);
  let w12 = clamp(log2(fp / F.lod.y), 0.0, 1.0);
  var lvl = 0u;
  if (xi < w01) { lvl = 1u; }
  if (xi < w12) { lvl = 2u; }
  return max(lvl, u32(F.lod.z));
}

// ---- the grid walk ---------------------------------------------------------------------------------------------------
// Things live in square cells and may overhang their cell by at most `ov`. The walk visits slabs
// along the ray's major horizontal axis; slab i owns the cells (i, j), and its extended range
// [i·C − ov, (i+1)·C + ov] gives the minor cells j whose contents the ray can meet. Each cell is
// visited once, slabs come near to far, and the walk stops once a slab starts beyond the limit.

struct Walk { ax: u32, om: f32, dm: f32, on: f32, dn: f32, i: i32, di: i32, cs: f32, ov: f32 }

fn walk_begin(o: vec3f, d: vec3f, t0: f32, cs: f32, ov: f32) -> Walk {
  var w: Walk;
  w.ax = select(1u, 0u, abs(d.x) >= abs(d.z));
  w.om = select(o.z, o.x, w.ax == 0u);
  w.dm = select(d.z, d.x, w.ax == 0u);
  w.on = select(o.x, o.z, w.ax == 0u);
  w.dn = select(d.x, d.z, w.ax == 0u);
  if (abs(w.dm) < 1e-6) { w.dm = select(-1e-6, 1e-6, w.dm >= 0.0); }
  w.di = select(-1, 1, w.dm > 0.0);
  w.cs = cs;
  w.ov = ov;
  let p = w.om + w.dm * t0;
  w.i = select(i32(floor((p + ov) / cs)), i32(floor((p - ov) / cs)), w.di > 0);
  return w;
}

/** The current slab: (t entry, t exit, first minor cell, last minor cell), clipped to [t0, t1]. */
fn walk_slab(w: Walk, t0: f32, t1: f32) -> vec4f {
  let ta = (f32(w.i) * w.cs - w.ov - w.om) / w.dm;
  let tb = (f32(w.i + 1) * w.cs + w.ov - w.om) / w.dm;
  let s0 = max(min(ta, tb), t0);
  let s1 = min(max(ta, tb), t1);
  let m0 = w.on + w.dn * s0;
  let m1 = w.on + w.dn * s1;
  return vec4f(s0, s1, floor((min(m0, m1) - w.ov) / w.cs), floor((max(m0, m1) + w.ov) / w.cs));
}

fn walk_cell(w: Walk, j: i32) -> vec2i { return select(vec2i(j, w.i), vec2i(w.i, j), w.ax == 0u); }

// ---- explicit leaves ---------------------------------------------------------------------------------------------------

struct LeafHit { t: f32, n: vec3f, tint: f32, ao: f32, tstop: f32 }

fn oct_dir(p: vec2f) -> vec3f {
  var n = vec3f(p, 1.0 - abs(p.x) - abs(p.y));
  if (n.z < 0.0) { n = vec3f((1.0 - abs(n.yx)) * select(vec2f(-1.0), vec2f(1.0), n.xy >= vec2f(0.0)), n.z); }
  return normalize(n);
}

/** First leaf along [ta, tb] (and before tlim). The crown's leaves live in two interleaved grids of
 *  cells, the second offset by half a cell so that its leaves fill the first grid's cell faces (a single
 *  grid shows comb-like gaps when seen edge-on). Two 3D DDAs advance together, always taking the cell
 *  whose entry is nearer. A cell's cluster is present where its presence draw falls under the crown
 *  profile (times the tile's gap factor) at the cell centre; each leaf is a planar shape hit exactly. */
fn leaf_walk(t: Tree, o: vec3f, d: vec3f, ta: f32, tb: f32, tlim: f32, budget: u32, c: ptr<function, Counters>) -> LeafHit {
  var res: LeafHit;
  res.t = INF;
  res.tstop = INF;
  let s = leaf_cell_m(t.kind);
  let org = crown_origin(t);
  let rt = leaf_rot(t);
  let rti = transpose(rt);
  let lo = rt * to_crown(t, o - org) / s + t.off;
  let ld = rt * to_crown(t, d) / s;
  let stp = vec3i(select(vec3i(-1), vec3i(1), ld >= vec3f(0.0)));
  let inv = 1.0 / select(ld, vec3f(1e-12), abs(ld) < vec3f(1e-12));
  let tdel = abs(inv);
  let pos = select(vec3f(0.0), vec3f(1.0), ld >= vec3f(0.0));
  var ca = vec3i(floor(lo + ld * ta));
  var ma = (vec3f(ca) + pos - lo) * inv;
  let lob = lo - 0.5;
  var cb = vec3i(floor(lob + ld * ta));
  var mb = (vec3f(cb) + pos - lob) * inv;
  var ina = ta;
  var inb = ta;
  let tend = min(tb, tlim);
  var best = INF;
  var bn = vec3f(0.0, 1.0, 0.0);
  var btint = 0.5;
  var bao = 1.0;
  let flutter = F.cr.w * 0.015;
  let egg = t.kind == 0u;
  let kbase = t.kind * 2u * LEAF_P3;
  var done = false;
  // The crown profile decides which cells hold a cluster. It's evaluated per 2×2×2 block of leaf
  // cells (at the block centre) and cached, so neighbouring cells share one evaluation; every ray
  // sees the same value for a given cell, so the leaves stay consistent between pixels.
  var blk = vec3i(1 << 30);
  var benv: Env;
  for (var n = 0u; n < budget; n++) {
    let useA = ina <= inb;
    let tin = select(inb, ina, useA);
    if (tin >= min(best, tend)) { done = true; break; }
    (*c).leaf_cells += 1u;
    let cell = select(cb, ca, useA);
    let mx = select(mb, ma, useA);
    let tout = min(mx.x, min(mx.y, mx.z));
    let go = select(0.5, 0.0, useA);
    let tc = vec3u(cell & vec3i(15));
    let bi = (kbase + select(LEAF_P3, 0u, useA) + tc.x + tc.y * LEAF_P + tc.z * LEAF_P * LEAF_P) * LEAF_U32;
    let head = unpack2x16unorm(leaves[bi]);
    let bc = vec3i(floor((vec3f(cell) + 0.5 + go) * 0.5));
    if (any(bc != blk)) { blk = bc; benv = crown_env(t, rti * ((vec3f(bc) * 2.0 + 1.0 - t.off) * s)); }
    let env = benv;
    if (head.x < env.prof * head.y) {
      let ro = lo - go - vec3f(cell);
      for (var k = 0u; k < LEAF_K; k++) {
        let A = unpack4x8unorm(leaves[bi + 1u + 3u * k]);          // centre xyz, a / 0.3
        let nrm = oct_dir(unpack2x16snorm(leaves[bi + 2u + 3u * k]));
        let C = unpack4x8unorm(leaves[bi + 3u + 3u * k]);          // tangent (oct), b / 0.3, tint
        var ctr = A.xyz;
        if (flutter > 0.0) { ctr += nrm * flutter * sin(F.cf.w * 6.0 + C.w * 40.0); }
        let den = dot(ld, nrm);
        if (abs(den) < 1e-9) { continue; }
        let tl = dot(ctr - ro, nrm) / den;
        if (tl < tin - 1e-4 || tl > tout + 1e-4 || tl >= best || tl >= tlim || tl <= 0.0) { continue; }
        let tv = oct_dir(C.xy * 2.0 - 1.0);
        let q = ro + ld * tl - ctr;
        let x = dot(q, tv) / (A.w * 0.3);
        let y = dot(q, cross(nrm, tv)) / (C.z * 0.3);
        let wy = select(1.0, 1.0 - 0.3 * x, egg);
        if (x * x + (y / wy) * (y / wy) < 1.0) { best = tl; bn = nrm; btint = C.w; bao = env.ao; }
      }
      (*c).leaf_tests += LEAF_K;
    }
    var m2 = mx;
    var c2 = cell;
    if (m2.x <= m2.y && m2.x <= m2.z) { c2.x += stp.x; m2.x += tdel.x; }
    else if (m2.y <= m2.z) { c2.y += stp.y; m2.y += tdel.y; }
    else { c2.z += stp.z; m2.z += tdel.z; }
    if (useA) { ca = c2; ma = m2; ina = tout; } else { cb = c2; mb = m2; inb = tout; }
  }
  if (!done) {
    // Out of budget: a cap hit if the budget was the cap, else the caller continues as a volume.
    if (budget >= cap_leaf_cells()) { (*c).cap_l += 1u; } else { res.tstop = min(ina, inb); }
  }
  if (best < INF) {
    res.t = best;
    res.n = from_crown(t, rti * bn);
    res.tint = btint;
    res.ao = bao;
  }
  return res;
}

// ---- crown bounds -------------------------------------------------------------------------------------------------------

/** The interval where a ray may meet the crown's leaves (the inflated envelope bound). */
fn crown_iv(t: Tree, o: vec3f, d: vec3f) -> vec2f {
  let org = crown_origin(t);
  if (t.kind == 0u) { return ellipsoid_iv(o, d, org, vec3f(t.rh, t.rv, t.rh)); }
  return cone_iv(o, d, org, t.rh * 1.3, t.rv);
}

/** The far level's homogeneous shape. */
fn far_iv(t: Tree, o: vec3f, d: vec3f) -> vec2f {
  let org = crown_origin(t);
  if (t.kind == 0u) { return ellipsoid_iv(o, d, org, vec3f(t.rh, t.rv, t.rh) * F.far.z); }
  return cone_iv(o, d, org, t.rh * F.far.w, t.rv);
}
/** The far level's extinction: calibrated for horizontal rays and rays 34° down (|d.y| = 0.565). */
fn far_sigma(kind: u32, ady: f32) -> f32 {
  return mix(select(F.far.x, F.far.y, kind == 1u), select(F.lod3.y, F.lod3.z, kind == 1u), clamp(ady / 0.565, 0.0, 1.0));
}

// ---- volumetric transmittance (shadows) ----------------------------------------------------------------------------------

/** Transmittance through the crown's volumetric texture along [ta, tb], footprint fp at ta. */
fn vol_trans(t: Tree, o: vec3f, d: vec3f, ta: f32, tb: f32, fp: f32, steps: f32, xi: f32, c: ptr<function, Counters>) -> f32 {
  let s = leaf_cell_m(t.kind);
  let ms = vol_setup(t.kind, tb - ta, fp, steps);
  let k = calib(t.kind, ms.x, abs(d.y)) / s;
  let org = crown_origin(t);
  let rt = leaf_rot(t);
  let lo = to_crown(t, o - org);
  let ld = to_crown(t, d);
  var tt = ta + xi * ms.y;
  var tau = 0.0;
  for (var n = 0u; n < cap_vol(); n++) {
    if (tt >= tb) { return exp(-tau); }
    (*c).vol += 1u;
    let l = lo + ld * tt;
    let env = crown_env(t, l);
    if (env.ef > 0.0) {
      tau += k * tex_at(t.kind, rt * l / s + t.off, ms.x).r * env.prof * ms.y;
      if (tau > 4.6) { return 0.0; }
    } else {
      tt += max(-env.ef * env_lip(t) - ms.y, 0.0);   // empty space: no density until the envelope
    }
    tt += ms.y;
  }
  (*c).cap_v += 1u;
  return exp(-tau);
}

// ---- trunks and branches ------------------------------------------------------------------------------------------------

/** Nearest bark hit (t, normal) on the trunk, its root flare, and (broadleaf) the four branches. */
fn bark_hit(t: Tree, o: vec3f, d: vec3f, tmin: f32, branches: bool, c: ptr<function, Counters>) -> vec4f {
  var best = vec4f(INF);
  if (dbg(DBG_BARK)) { return best; }
  let base = t.base;
  let top = select(base + t.cy + 0.55 * t.rv, base + t.cy, t.kind == 0u);
  let rtop = select(0.05, t.r0 * 0.45, t.kind == 0u);
  // Cheap pre-test: does the ray pass within the flare's radius of the trunk axis at all?
  let tiv = circle_xz(o, d, t.pos, t.r0 * 1.7);
  if (tiv.x <= tiv.y && tiv.y >= tmin) {
    (*c).trunk += 1u;
    best = frustum_hit(o, d, t.pos, base - 0.3, top, t.r0, rtop, tmin);
    let flare = frustum_hit(o, d, t.pos, base - 0.3, base + 0.9, t.r0 * 1.7, t.r0 * 0.93, tmin);
    if (flare.x < best.x) { best = flare; }
  }
  if (branches && t.kind == 0u && t.slot == 0u) {
    for (var k = 0u; k < 4u; k++) {
      (*c).branch += 1u;
      let b = branch(t, k);
      let h = capsule_hit(o, d, b[0].xyz, b[1].xyz, b[0].w, tmin);
      if (h.x < best.x) { best = h; }
    }
  }
  return best;
}
