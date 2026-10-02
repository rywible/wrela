// Primary visibility: terrain, the floor layer (grass, ferns), then trees near to far. Writes a
// two-layer G-buffer: the nearest opaque surface, and one composited foliage layer in front of it.
// Appended to common.wgsl + foliage.wgsl.

@group(0) @binding(5) var gA: texture_storage_2d<rgba32uint, write>;
@group(0) @binding(6) var gB: texture_storage_2d<rg32uint, write>;
@group(0) @binding(7) var<storage, read_write> stats: array<atomic<u32>, 64>;

override TREES: bool = true;
override FLOOR: bool = true;

const G_CELL: f32 = 0.15;       // grass cell (m)
const G_OV: f32 = 0.05;         // blade overhang: tip inside the cell + wind 0.035 + half width
const P_CELL: f32 = 1.3;        // fern cell
const P_OV: f32 = 0.45;
const BLADE_W: f32 = 0.012;     // mean blade width at the base
const FROND_W: f32 = 0.08;      // typical frond width

fn terrain_cap() -> u32 { return select(160u, 2048u, REF); }
fn floor_cap() -> u32 { return select(96u, 1200u, REF); }

struct Hit { t: f32, mat: u32, alb: vec3f, n: vec3f, ao: f32, trans: f32 }

struct Fol { T: f32, alb: vec3f, conif: f32, ao: f32, tw: f32, w: f32 }

// ---- terrain: a heightfield traced with a directional Lipschitz bound (spike 02), three levels ----------------------
// Level 0 finds where the ray enters the canopy layer (terrain + CANOPY), level 1 the floor layer
// (terrain + SHELL), level 2 the ground. Each level is approached from above, so none is skipped.

fn terrain_march(o: vec3f, d: vec3f, tmax: f32, pf: f32, c: ptr<function, Counters>) -> vec3f {
  var res = vec3f(INF);
  let rate = -d.y + T_SLOPE * length(d.xz);
  var t = 0.0;
  var gap = o.y - terrain_h(o.x, o.z);
  var lvl = 0u;
  if (gap < CANOPY) { res.x = 0.0; lvl = 1u; }
  if (gap < SHELL) { res.y = 0.0; lvl = 2u; }
  if (rate <= 0.0) { return res; }
  let step_scale = select(1.0, 0.5, REF);
  var t_prev = 0.0;
  var g_prev = INF;
  for (var i = 0u; i < terrain_cap(); i++) {
    let lvl_h = select(select(0.0, SHELL, lvl == 1u), CANOPY, lvl == 0u);
    let g = gap - lvl_h;
    if (lvl == 2u) {
      let eps = max(select(0.25, 0.02, REF) * t * pf, 1e-4);
      if (g < eps) {
        if (g_prev < INF && g_prev > g) { t += g * (t - t_prev) / (g_prev - g); }
        res.z = t;
        return res;
      }
    } else if (g < 0.02) {
      if (lvl == 0u) { res.x = t; } else { res.y = t; }
      lvl += 1u;
      continue;
    }
    (*c).steps += 1u;
    t_prev = t;
    g_prev = g;
    t += g / rate * step_scale;
    if (t > tmax) { return res; }
    gap = o.y + d.y * t - terrain_h(o.x + d.x * t, o.z + d.z * t);
  }
  (*c).cap_t += 1u;
  if (lvl == 2u) { res.z = t; }
  return res;
}

// ---- the floor: grass blades and ferns --------------------------------------------------------------------------------

fn wind_lean(p: vec2f) -> vec2f {
  let tm = F.cf.w;
  let g = 0.6 + 0.4 * sin(1.7 * dot(p, WIND_DIR) - 2.6 * tm + 0.8 * sin(0.31 * p.x + 0.23 * p.y));
  return WIND_DIR * (F.cr.w * 0.035 * g);
}

/** Expected optical depth of the explicit blades along a ray segment that descends through the
 *  floor layer from height y_in above the ground to the ground over horizontal length lh: vertical
 *  planar blades at random azimuth project (2/π) of their width, and a blade's width at height y is
 *  w·sqrt(1 − y/H), so ∫w dy = (2/3)·w·H·(1 − (1 − Y/H)^1.5). */
fn grass_tau(mask: f32, y_in: f32, lh: f32) -> f32 {
  let n = F.world.y * mask / (G_CELL * G_CELL);
  let hm = 0.33 * (0.55 + 0.45 * mask);
  let yy = min(y_in, hm);
  let integ = (2.0 / 3.0) * BLADE_W * hm * (1.0 - pow(max(1.0 - yy / hm, 0.0), 1.5));
  return n * (2.0 / PI) * integ * lh / max(y_in, 1e-3);
}

fn grass_walk(o: vec3f, d: vec3f, t0: f32, t1: f32, hit: ptr<function, Hit>, c: ptr<function, Counters>) {
  var w = walk_begin(o, d, t0, G_CELL, G_OV);
  var lat = vec2f(1e9);
  var lh = vec3f(0.0);
  var lm = 0.0;
  let dxz2 = dot(d.xz, d.xz);
  let cheap = dxz2 > 0.0025;
  let slope = abs(d.y) * inverseSqrt(max(dxz2, 1e-8));
  for (var k = 0u; k < floor_cap(); k++) {
    let lim = min(t1, (*hit).t);
    let s = walk_slab(w, t0, lim);
    if (s.x > lim) { return; }
    (*c).gslabs += 1u;
    if (s.y >= s.x) {
      let jn = i32(s.w - s.z) + 1;
      for (var jj = 0; jj < jn; jj++) {
        let j = select(i32(s.w) - jj, i32(s.z) + jj, w.dn >= 0.0);
        let ci = walk_cell(w, j);
        (*c).gcells += 1u;
        let corner = vec2f(ci) * G_CELL;
        // Root heights (first-order Taylor) and the grass mask come from the nearest 1 m lattice
        // point, so every ray agrees on every blade.
        let lp = round(corner + 0.5 * G_CELL);
        if (any(lp != lat)) { lat = lp; lh = terrain_hg(lp.x, lp.y); lm = grass_mask(lp.x, lp.y); }
        let mask = lm;
        if (mask < 0.02) { continue; }
        let h = pcg3(vec3u(bitcast<u32>(ci.x), bitcast<u32>(ci.y), 0x6A09E667u));
        let nb = u32(F.world.y);
        for (var b = 0u; b < nb; b++) {
          let hb = pcg3(h + vec3u(b * 0x9E3779B9u, b * 0x85EBCA6Bu, b));
          let hb2 = pcg3(hb.zxy);
          if (u01(hb.x) >= mask) { continue; }
          (*c).blades += 1u;
          let lean = (vec2f(u01(hb2.x), u01(hb2.y)) - 0.5) * 0.12;
          let room = G_CELL - abs(lean);
          let rxz = corner + max(-lean, vec2f(0.0)) + vec2f(u01(hb.y), u01(hb.z)) * room;
          let H = (0.16 + 0.34 * u01(hb2.z)) * (0.55 + 0.45 * mask);
          let ry = lh.x + lh.y * (rxz.x - lat.x) + lh.z * (rxz.y - lat.y);
          let root = vec3f(rxz.x, ry, rxz.y);
          let tipo = lean + wind_lean(rxz);
          if (cheap) {
            // Cheap rejection: where the ray passes the blade's middle in xz, is it near and low enough?
            let rel = rxz + 0.5 * tipo - o.xz;
            let tq = dot(rel, d.xz) / dxz2;
            let off = rel - d.xz * tq;
            let rr = 0.5 * length(tipo) + 0.012;
            if (dot(off, off) > rr * rr) { continue; }
            if (o.y + d.y * tq > ry + H + slope * rr + 0.01) { continue; }
          }
          let ax = vec3f(tipo.x, H, tipo.y);
          let fa = vec2f(u01(hb2.x ^ hb.y), u01(hb2.y ^ hb.z)) - 0.5;
          let wdir = vec3f(normalize(fa + vec2f(1e-4, 0.0)), 0.0).xzy;
          let nrm = cross(wdir, ax);
          let den = dot(d, nrm);
          if (abs(den) < 1e-9) { continue; }
          let tt = dot(root - o, nrm) / den;
          if (tt <= t0 || tt >= (*hit).t) { continue; }
          let q = o + d * tt - root;
          let ap = ax - wdir * dot(ax, wdir);
          let v = dot(q, ap) / dot(ax, ap);
          if (v < 0.0 || v > 1.0) { continue; }
          let u = dot(q - ax * v, wdir);
          let w0 = BLADE_W * (0.75 + 0.5 * u01(hb.x ^ hb2.z));
          if (abs(u) > 0.5 * w0 * sqrt(1.0 - v)) { continue; }
          var n = normalize(nrm);
          if (dot(n, d) > 0.0) { n = -n; }
          // Blades curve across their width: tilt the shading normal toward the blade's edge.
          n = normalize(n + wdir * (u / (0.5 * w0)) * 0.35 + vec3f(0.0, 0.15, 0.0));
          let tint = u01(hb2.z ^ hb.x);
          let dry = select(0.0, 1.0, tint > 0.9);
          let green = mix(vec3f(0.030, 0.062, 0.010), vec3f(0.115, 0.165, 0.030), smoothstep(0.0, 0.7, v));
          let tip = mix(green, vec3f(0.19, 0.19, 0.055), smoothstep(0.6, 1.0, v) * 0.6);
          (*hit).t = tt;
          (*hit).mat = MAT_GRASS;
          (*hit).alb = mix(tip * (0.75 + 0.5 * tint), vec3f(0.24, 0.19, 0.09), dry);
          (*hit).n = n;
          (*hit).ao = mix(0.25, 1.0, pow(v, 0.8));
          (*hit).trans = 0.55;
        }
      }
    }
    w.i += w.di;
  }
  (*c).cap_g += 1u;
}

fn fern_walk(o: vec3f, d: vec3f, t0: f32, t1: f32, hit: ptr<function, Hit>, c: ptr<function, Counters>) {
  var w = walk_begin(o, d, t0, P_CELL, P_OV);
  let rot = vec2f(0.62349, 0.78183);   // cos, sin of 2π/7
  for (var k = 0u; k < floor_cap(); k++) {
    let lim = min(t1, (*hit).t);
    let s = walk_slab(w, t0, lim);
    if (s.x > lim) { return; }
    if (s.y >= s.x) {
      let jn = i32(s.w - s.z) + 1;
      for (var jj = 0; jj < jn; jj++) {
        let j = select(i32(s.w) - jj, i32(s.z) + jj, w.dn >= 0.0);
        let ci = walk_cell(w, j);
        (*c).fcells += 1u;
        let h = pcg3(vec3u(bitcast<u32>(ci.x), bitcast<u32>(ci.y), 0xBB67AE85u));
        let ctr = (vec2f(ci) + 0.3 + 0.4 * vec2f(u01(h.y), u01(h.z))) * P_CELL;
        if (u01(h.x) >= fern_mask(ctr.x, ctr.y) * F.world.z) { continue; }
        let civ = circle_xz(o, d, ctr, 0.85);
        if (civ.y < t0 || civ.x > (*hit).t || civ.x > civ.y) { continue; }
        let h2 = pcg3(h.yzx);
        let by = terrain_h(ctr.x, ctr.y) - 0.02;
        if (min(o.y + d.y * civ.x, o.y + d.y * civ.y) > by + 0.8) { continue; }
        let scale = 0.6 + 0.5 * u01(h2.x);
        let a0 = u01(h2.y) * 6.2831853;
        var az = vec2f(cos(a0), sin(a0));
        let root = vec3f(ctr.x, by, ctr.y);
        let sway = WIND_DIR * (F.cr.w * 0.03 * sin(1.9 * F.cf.w + u01(h2.z) * 6.28));
        for (var f = 0u; f < 7u; f++) {
          (*c).fronds += 1u;
          let hf = pcg3(h2 + vec3u(f * 0x9E3779B9u, f, f * 0x632BE5ABu));
          let az2 = normalize(az + (vec2f(u01(hf.x), u01(hf.y)) - 0.5) * 0.35);
          az = vec2f(az.x * rot.x - az.y * rot.y, az.x * rot.y + az.y * rot.x);
          let st = mix(0.55, 0.9, u01(hf.z));
          let ct = sqrt(1.0 - st * st);
          let L = scale * (0.45 + 0.3 * u01(hf.x ^ hf.y));
          let ax = normalize(vec3f(az2.x * st + sway.x, ct, az2.y * st + sway.y));
          let wd = vec3f(-az2.y, 0.0, az2.x);
          let nrm = cross(ax, wd);
          let den = dot(d, nrm);
          if (abs(den) < 1e-9) { continue; }
          let tt = dot(root - o, nrm) / den;
          if (tt <= t0 || tt >= (*hit).t) { continue; }
          let q = o + d * tt - root;
          let x = dot(q, ax);
          if (x < 0.0 || x > L) { continue; }
          let xf = x / L;
          let leaflet = 0.55 + 0.45 * abs(sin(x * 48.0));
          let hw = FROND_W * 0.5 * scale * 4.0 * xf * (1.0 - xf) * leaflet * 1.6;
          let y = dot(q, wd);
          if (abs(y) > hw) { continue; }
          var n = normalize(nrm);
          if (dot(n, d) > 0.0) { n = -n; }
          (*hit).t = tt;
          (*hit).mat = MAT_FERN;
          (*hit).alb = mix(vec3f(0.035, 0.075, 0.014), vec3f(0.085, 0.140, 0.025), xf) * (0.8 + 0.4 * u01(hf.y ^ hf.z));
          (*hit).n = normalize(n + vec3f(0.0, 0.2, 0.0));
          (*hit).ao = mix(0.35, 1.0, xf);
          (*hit).trans = 0.5;
        }
      }
    }
    w.i += w.di;
  }
  (*c).cap_f += 1u;
}

// ---- ground colour ------------------------------------------------------------------------------------------------------

fn vnoise2(p: vec2f) -> f32 {
  let i = floor(p);
  let f = p - i;
  let u = f * f * (3.0 - 2.0 * f);
  let ii = vec2u(vec2i(i) + vec2i(4096));
  let a = u01(pcg3(vec3u(ii.x, ii.y, 11u)).x);
  let b = u01(pcg3(vec3u(ii.x + 1u, ii.y, 11u)).x);
  let cc = u01(pcg3(vec3u(ii.x, ii.y + 1u, 11u)).x);
  let dd = u01(pcg3(vec3u(ii.x + 1u, ii.y + 1u, 11u)).x);
  return mix(mix(a, b, u.x), mix(cc, dd, u.x), u.y);
}

fn ground_albedo(p: vec3f, fp: f32) -> vec3f {
  let n1 = vnoise2(p.xz * 0.35);
  let n2 = vnoise2(p.xz * 1.7 + 17.0);
  let n3 = select(vnoise2(p.xz * 7.0 + 41.0), 0.5, fp > 0.2);
  let litter = mix(vec3f(0.070, 0.045, 0.025), vec3f(0.115, 0.075, 0.040), n2) * (0.8 + 0.4 * n3);
  let moss = mix(vec3f(0.035, 0.055, 0.015), vec3f(0.060, 0.080, 0.020), n3);
  return mix(litter, moss, smoothstep(0.45, 0.75, n1));
}

// ---- trees ------------------------------------------------------------------------------------------------------------

fn trace_tree(ci: vec2i, slot: u32, o: vec3f, d: vec3f, t0: f32, pf: f32, px: vec2u, hit: ptr<function, Hit>, fol: ptr<function, Fol>, c: ptr<function, Counters>) {
  (*c).cells += 1u;
  let tl = tree_lite(ci, slot);
  if (!tl.exists) { return; }
  let lim = (*hit).t;
  let iv0 = circle_xz(o, d, tl.pos, tl.rb);
  if (iv0.x > iv0.y || iv0.y < t0 || iv0.x > lim) { return; }
  let ya0 = o.y + d.y * max(iv0.x, t0);
  let yb0 = o.y + d.y * min(iv0.y, lim);
  if (min(ya0, yb0) > tl.top || max(ya0, yb0) < tl.base - 0.5) { return; }
  let t = tree_full(tl, ci, slot);
  let iv = circle_xz(o, d, t.pos, tree_bound_r(t));
  if (iv.x > iv.y || iv.y < t0 || iv.x > lim) { return; }
  let ya = o.y + d.y * max(iv.x, t0);
  let yb = o.y + d.y * min(iv.y, lim);
  if (min(ya, yb) > t.base + t.height + 0.5 || max(ya, yb) < t.base - 0.5) { return; }
  (*c).trees += 1u;

  let civ = crown_iv(t, o, d);
  let ta = max(civ.x, t0);
  let in_crown = civ.x <= civ.y && ta < min(civ.y, lim);
  var lvl = 2u;
  if (in_crown) { lvl = choose_level(t, max(ta, 0.05) * pf, prand(px, t.id)); }

  // Bark: trunk and flare always, branches when the crown is drawn as leaves or texture.
  let bh = bark_hit(t, o, d, t0, in_crown && lvl <= 1u, c);
  if (bh.x < (*hit).t) {
    let bp = o + d * bh.x;
    let rel = bp.xz - t.pos;
    let ang = atan2(rel.y, rel.x);
    // Bark: vertical furrows (noise stretched along the trunk); beech-grey broadleaves, red-brown conifers.
    let fur = vnoise2(vec2f(ang * select(4.0, 7.0, t.kind == 1u) + f32(t.id & 63u), (bp.y - t.base) * select(0.35, 0.9, t.kind == 1u)));
    let fine = vnoise2(vec2f(ang * 19.0, (bp.y - t.base) * 3.0) + 7.0);
    var bc = select(vec3f(0.20, 0.19, 0.165), vec3f(0.125, 0.075, 0.050), t.kind == 1u);
    bc *= 0.55 + 0.6 * fur * (0.8 + 0.4 * fine);
    let moss = smoothstep(1.4, 0.0, bp.y - t.base) * 0.7 + 0.25 * smoothstep(0.6, 0.9, fine) * f32(t.kind == 0u);
    (*hit).t = bh.x;
    (*hit).mat = MAT_BARK;
    (*hit).alb = mix(bc, vec3f(0.05, 0.08, 0.02), clamp(moss, 0.0, 1.0));
    let tang = vec3f(-rel.y, 0.0, rel.x) / max(length(rel), 1e-4);
    (*hit).n = normalize(bh.yzw + tang * (fur - 0.5) * 0.8);
    (*hit).ao = select(0.8, 0.55, bp.y > t.base + t.cy);
    (*hit).trans = 0.0;
  }
  if (!in_crown || dbg(DBG_CROWN)) { return; }
  let tb = min(civ.y, (*hit).t);
  if (ta >= tb) { return; }

  var va = ta;   // where a volume march would start
  if (!REF && lvl <= 1u) {
    // Skip to the envelope. The margin covers the leaf walk's block-centred presence test (a block
    // whose centre is inside can reach 1.9 cells outside).
    va = env_entry(t, o, d, ta, tb, 2.2 * leaf_cell_m(t.kind));
    if (va >= tb) { return; }
  }
  if (lvl == 0u) {
    (*c).l0 += 1u;
    // Explicit leaves for a bounded number of cells from where the ray enters the crown; if none is hit
    // by then, the rest of the chord (the crown's interior, seen through gaps) continues as the volume.
    let budget = select(u32(F.lod3.x), cap_leaf_cells(), REF);
    let lh = leaf_walk(t, o, d, va, tb, (*hit).t, budget, c);
    if (lh.t < (*hit).t) {
      var n = lh.n;
      if (dot(n, d) > 0.0) { n = -n; }
      (*hit).t = lh.t;
      (*hit).mat = select(MAT_LEAF, MAT_NEEDLE, t.kind == 1u);
      (*hit).alb = foliage_albedo(t, lh.tint);
      (*hit).n = n;
      (*hit).ao = lh.ao;
      (*hit).trans = select(0.55, 0.25, t.kind == 1u);
      return;
    }
    if (lh.tstop >= tb) { return; }
    va = lh.tstop;
    (*c).l0v += 1u;
  }

  let org = crown_origin(t);
  if (lvl == 2u) {
    // Density volume: a homogeneous ellipsoid or cone, Beer–Lambert along the chord.
    (*c).l2 += 1u;
    let fv = far_iv(t, o, d);
    let fa = max(fv.x, t0);
    let fb = min(fv.y, (*hit).t);
    if (fa >= fb) { return; }
    let sg = far_sigma(t.kind, abs(d.y));
    let a = 1.0 - exp(-sg * (fb - fa));
    let trep = fa + min(fb - fa, 0.69 / sg);
    let wgt = (*fol).T * a;
    (*fol).alb += wgt * foliage_albedo(t, 0.5);
    (*fol).conif += wgt * f32(t.kind);
    (*fol).ao += wgt * crown_env(t, to_crown(t, o + d * trep - org)).ao;
    (*fol).tw += wgt * trep;
    (*fol).w += wgt;
    (*fol).T *= 1.0 - a;
    return;
  }

  // Volumetric texture: march the crown, compositing front to back.
  if (lvl == 1u) { (*c).l1 += 1u; }
  let s = leaf_cell_m(t.kind);
  // A continuation after explicit leaves is near: twice the steps, so its mip stays finer.
  let ms = vol_setup(t.kind, tb - va, max(va, 0.05) * pf, F.lod2.z * select(1.0, 2.0, lvl == 0u));
  let kc = calib(t.kind, ms.x, abs(d.y)) / s;
  let rt = leaf_rot(t);
  let lo = to_crown(t, o - org);
  let ld = to_crown(t, d);
  var tt = va + prand(px, t.id ^ 0x5bd1e995u) * ms.y;
  for (var n = 0u; n < cap_vol(); n++) {
    if (tt >= tb) { return; }
    (*c).vol += 1u;
    let l = lo + ld * tt;
    let env = crown_env(t, l);
    if (env.ef > 0.0) {
      let tx = tex_at(t.kind, rt * l / s + t.off, ms.x);
      let a = 1.0 - exp(-kc * tx.r * env.prof * ms.y);
      if (a > 0.0) {
        let inv = 1.0 / max(tx.r, 1e-6);
        let wgt = (*fol).T * a;
        (*fol).alb += wgt * foliage_albedo(t, tx.a * inv);
        (*fol).conif += wgt * f32(t.kind);
        (*fol).ao += wgt * env.ao;
        (*fol).tw += wgt * tt;
        (*fol).w += wgt;
        (*fol).T *= 1.0 - a;
        if ((*fol).T < 0.01) { return; }
      }
    } else {
      tt += max(-env.ef * env_lip(t) - ms.y, 0.0);   // empty space: no density until the envelope
    }
    tt += ms.y;
  }
  (*c).cap_v += 1u;
}

fn tree_walk(o: vec3f, d: vec3f, t0: f32, t1: f32, pf: f32, px: vec2u, hit: ptr<function, Hit>, fol: ptr<function, Fol>, c: ptr<function, Counters>) {
  var w = walk_begin(o, d, t0, TREE_C, TREE_OV);
  for (var k = 0u; k < cap_slabs(); k++) {
    let lim = min(t1, (*hit).t);
    let s = walk_slab(w, t0, lim);
    if (s.x > lim || (*fol).T < 0.01) { return; }
    (*c).slabs += 1u;
    if (s.y >= s.x) {
      let jn = i32(s.w - s.z) + 1;
      for (var jj = 0; jj < jn; jj++) {
        let j = select(i32(s.w) - jj, i32(s.z) + jj, w.dn >= 0.0);
        let ci = walk_cell(w, j);
        trace_tree(ci, 0u, o, d, t0, pf, px, hit, fol, c);
        if (!dbg(DBG_UNDER)) { trace_tree(ci, 1u, o, d, t0, pf, px, hit, fol, c); }
      }
    }
    w.i += w.di;
  }
  (*c).cap_s += 1u;
}

// ---- the kernel ------------------------------------------------------------------------------------------------------

fn heat(x: f32) -> vec3f {
  let t = clamp(x, 0.0, 1.0);
  return clamp(vec3f(1.5 * t, 1.5 * t * t, 0.5 - t) + vec3f(0.0, 0.0, 0.2) * (1.0 - t), vec3f(0.0), vec3f(1.0));
}

var<workgroup> wsum: array<atomic<u32>, 32>;

@compute @workgroup_size(8, 8)
fn trace(@builtin(global_invocation_id) gid: vec3u, @builtin(local_invocation_index) li: u32) {
  var c: Counters;
  var hit: Hit;
  var fol: Fol;
  let tp = tile_px(gid);
  let px = tp.xy;
  let valid = tp.z == 1u;
  if (valid) {
    var jit = F.jit.xy;
    if (REF) { jit = vec2f(prand(px, 0x1234u), prand(px, 0x5678u)) - 0.5; }
    let d = ray_dir(vec2f(px) + 0.5 + jit);
    let o = F.eye.xyz;
    let pf = F.eye.w;
    let tview = F.jit.z;
    hit.t = INF; hit.mat = MAT_SKY; hit.alb = vec3f(0.0); hit.n = vec3f(0.0, 1.0, 0.0); hit.ao = 1.0; hit.trans = 0.0;
    fol.T = 1.0;

    let tm = terrain_march(o, d, tview, pf, &c);
    if (tm.z < INF) {
      let p = o + d * tm.z;
      hit.t = tm.z;
      hit.mat = MAT_GROUND;
      hit.alb = ground_albedo(p, tm.z * pf);
      hit.n = terrain_n(p.x, p.z);
      hit.ao = 1.0;
    }

    // The floor layer. Explicit blades and fronds out to a per-pixel switch distance (the footprint
    // ramp, sampled stochastically); beyond it, the ground takes their expected cover.
    if (FLOOR && F.iso.w == 0 && tm.y < tview && tm.z < INF) {
      let xg = prand(px, 0x9e3779b9u);
      let tsw_g = select(F.lod2.x * BLADE_W / pf * exp2(xg), INF, REF);
      let tsw_f = select(F.lod2.y * FROND_W / pf * exp2(xg), INF, REF);
      let ts = max(tm.y - 0.05, 0.0);
      if (!dbg(DBG_FERN)) { fern_walk(o, d, ts, min(tm.z, tsw_f), &hit, &c); }
      // Grass is at most 0.5 m tall: start its walk where the ray is 0.62 m above the ground (a linear
      // guess between the 0.8 m shell entry and the ground, refined by one secant step).
      var tg = ts;
      if (tm.y > 0.0) {
        let te = tm.z - (tm.z - tm.y) * (0.62 / SHELL);
        let pe = o + d * te;
        let ge = pe.y - terrain_h(pe.x, pe.z);
        tg = clamp(te + (ge - 0.62) * (te - tm.y) / max(SHELL - ge, 1e-3) - 0.05, ts, tm.z);
      }
      let tge = min(min(tm.z, tsw_g), hit.t);
      let pa = o + d * tg;
      let pb = o + d * tge;
      let mlen = length(pb.xz - pa.xz);
      if (!dbg(DBG_GRASS) && tge > tg && max(grass_mask(pa.x, pa.z), grass_mask(pb.x, pb.z)) + 0.12 * 0.5 * mlen > 0.02) {
        grass_walk(o, d, tg, tge, &hit, &c);
      }
      if (hit.mat == MAT_GROUND) {
        let p = o + d * hit.t;
        // Statistical cover beyond the switch distances.
        let ys = o.y + d.y * max(tsw_g, ts) - (p.y);
        if (tsw_g < hit.t) {
          let gm = grass_mask(p.x, p.z);
          let lh = length(d.xz) * (hit.t - max(tsw_g, ts)) * select(0.0, 1.0, gm > 0.0);
          // The light pass shades this cover as the blades' own thin model, averaged (light.wgsl).
          hit.trans = 1.0 - exp(-grass_tau(gm, clamp(ys, 0.0, SHELL), lh));
        }
        if (tsw_f < hit.t) {
          let fc = 0.55 * fern_mask(p.x, p.z) * F.world.z;
          hit.alb = mix(hit.alb, vec3f(0.06, 0.11, 0.02), fc);
          hit.ao = min(hit.ao, mix(1.0, 0.75, fc));
        }
      }
    }

    if (TREES && tm.x < INF) {
      // Trees live below terrain + CANOPY: start where the ray enters that layer, stop where it leaves.
      var t1 = min(hit.t, tview);
      if (d.y > 0.0) { t1 = min(t1, (T_HMAX + CANOPY - o.y) / d.y); }
      if (t1 > tm.x) { tree_walk(o, d, max(tm.x - 1.0, 0.0), t1, pf, px, &hit, &fol, &c); }
    }

    // Pack the G-buffer.
    var af = 1.0 - fol.T;
    var fa = vec3f(0.0);
    var fcon = 0.0;
    var fao = 1.0;
    var ft = 0.0;
    if (fol.w > 1e-5) {
      fa = fol.alb / fol.w;
      fcon = fol.conif / fol.w;
      fao = fol.ao / fol.w;
      ft = fol.tw / fol.w;
      if (fol.T < 0.01) { af = 1.0; }
    } else { af = 0.0; }
    var mat = hit.mat;
    var oalb = hit.alb;
    if (VIEW == 1u) {
      let work = f32(c.steps + c.cells + 3u * c.trees + c.leaf_cells + c.leaf_tests / 2u + c.gcells + c.blades / 2u
                 + c.fcells + c.fronds / 2u + 2u * c.vol + c.trunk + c.branch);
      mat = MAT_DEBUG;
      oalb = heat(work / 300.0);
      af = 0.0;
    }
    textureStore(gA, vec2i(px), vec4u(bitcast<u32>(hit.t), pack2x16float(vec2f(min(ft, 65000.0), af)),
      alb_enc(oalb, f32(mat) / 255.0), alb_enc(fa, fao)));
    textureStore(gB, vec2i(px), vec4u(oct_enc(hit.n) | (u32(clamp(hit.ao, 0.0, 1.0) * 255.0) << 24u),
      u32(clamp(fcon, 0.0, 1.0) * 255.0) | (u32(clamp(hit.trans, 0.0, 1.0) * 255.0) << 24u), 0u, 0u));
  }

  if (STATS) {
    if (li < 32u) { atomicStore(&wsum[li], 0u); }
    workgroupBarrier();
    if (valid) {
      let caps = c.cap_t + c.cap_s + c.cap_l + c.cap_v + c.cap_g + c.cap_f;
      if (hit.mat < 7u) { atomicAdd(&wsum[hit.mat], 1u); }
      atomicAdd(&wsum[7], c.l0v);
      if (1.0 - fol.T > 0.5) { atomicAdd(&wsum[8], 1u); }
      atomicAdd(&wsum[9], c.steps);
      atomicAdd(&wsum[10], c.slabs);
      atomicAdd(&wsum[11], c.cells);
      atomicAdd(&wsum[12], c.trees);
      atomicAdd(&wsum[13], c.l0);
      atomicAdd(&wsum[14], c.l1);
      atomicAdd(&wsum[15], c.l2);
      atomicAdd(&wsum[16], c.leaf_cells);
      atomicAdd(&wsum[17], c.leaf_tests);
      atomicAdd(&wsum[18], c.vol);
      atomicAdd(&wsum[19], c.gslabs);
      atomicAdd(&wsum[20], c.gcells);
      atomicAdd(&wsum[21], c.blades);
      atomicAdd(&wsum[22], c.fcells);
      atomicAdd(&wsum[23], c.fronds);
      atomicAdd(&wsum[24], c.trunk);
      atomicAdd(&wsum[25], c.branch);
      atomicAdd(&wsum[26], c.cap_t);
      atomicAdd(&wsum[27], c.cap_s);
      atomicAdd(&wsum[28], c.cap_l);
      atomicAdd(&wsum[29], c.cap_v);
      atomicAdd(&wsum[30], c.cap_g + c.cap_f);
      if (caps > 0u) { atomicAdd(&wsum[31], 1u); }
    }
    workgroupBarrier();
    if (li < 32u) { atomicAdd(&stats[li], atomicLoad(&wsum[li])); }
  }
}
