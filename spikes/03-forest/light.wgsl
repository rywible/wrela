// Lighting: a sun-shadow ray from each G-buffer layer, cone-traced through the crowns; sky occlusion
// from nearby crowns; two-sided leaf shading with translucency; a foliage phase function for the
// volumes; sky, aerial perspective and tone mapping. Appended to common.wgsl + foliage.wgsl.

@group(0) @binding(5) var gA: texture_2d<u32>;
@group(0) @binding(6) var gB: texture_2d<u32>;
@group(0) @binding(7) var<storage, read_write> stats: array<atomic<u32>, 64>;
@group(0) @binding(8) var out_tex: texture_storage_2d<rgba16float, write>;

override SHADOWS: bool = true;

const SUN_C = vec3f(3.3, 2.95, 2.45);
const EXPOSURE: f32 = 1.35;

struct LC { rays: u32, slabs: u32, cells: u32, trees: u32, ao: u32, caps: u32 }

// ---- sun visibility -------------------------------------------------------------------------------------------

/** Transmittance toward the sun from p. Trunks and branches are opaque. Crowns use the volumetric
 *  texture at the mip the sun's cone selects (fp0 + t·sun diameter), or the far density volume; the
 *  reference instead tests explicit leaves against a direction sampled over the sun's disc. */
fn sun_vis(p: vec3f, fp0: f32, px: vec2u, salt: u32, c: ptr<function, Counters>, lc: ptr<function, LC>) -> f32 {
  var L = F.sun.xyz;
  if (REF) {
    let up = select(vec3f(0.0, 1.0, 0.0), vec3f(1.0, 0.0, 0.0), abs(L.y) > 0.9);
    let e1 = normalize(cross(L, up));
    let e2 = cross(L, e1);
    let r = sqrt(prand(px, salt ^ 0x27d4eb2fu)) * tan(F.sun.w);
    let a = prand(px, salt ^ 0x165667b1u) * 6.2831853;
    L = normalize(L + (e1 * cos(a) + e2 * sin(a)) * r);
  }
  (*lc).rays += 1u;
  let o = p + L * select(0.05, 0.005, REF);
  let t1 = (T_HMAX + CANOPY - o.y) / L.y;
  if (t1 <= 0.0) { return 1.0; }
  var T = 1.0;
  var w = walk_begin(o, L, 0.0, TREE_C, TREE_OV);
  for (var k = 0u; k < cap_slabs(); k++) {
    let s = walk_slab(w, 0.0, t1);
    if (s.x > t1) { return T; }
    (*lc).slabs += 1u;
    if (s.y >= s.x) {
      let jn = i32(s.w - s.z) + 1;
      for (var jj = 0; jj < jn; jj++) {
        let j = select(i32(s.w) - jj, i32(s.z) + jj, w.dn >= 0.0);
        let ci = walk_cell(w, j);
        for (var slot = 0u; slot < select(2u, 1u, dbg(DBG_UNDER)); slot++) {
          (*lc).cells += 1u;
          if (dbg(DBG_SHADOW_TREES)) { break; }
          let tl = tree_lite(ci, slot);
          if (!tl.exists) { continue; }
          let iv0 = circle_xz(o, L, tl.pos, tl.rb);
          if (iv0.x > iv0.y || iv0.y < 0.0 || iv0.x > t1) { continue; }
          if (o.y + L.y * max(iv0.x, 0.0) > tl.top) { continue; }
          let t = tree_full(tl, ci, slot);
          let iv = circle_xz(o, L, t.pos, tree_bound_r(t));
          if (iv.x > iv.y || iv.y < 0.0 || iv.x > t1) { continue; }
          if (o.y + L.y * max(iv.x, 0.0) > t.base + t.height + 0.5) { continue; }
          (*lc).trees += 1u;
          let civ = crown_iv(t, o, L);
          let ta = max(civ.x, 0.0);
          let in_crown = civ.x <= civ.y && ta < civ.y;
          let fp = fp0 + ta * 2.0 * F.sun.w;
          var lvl = 0u;
          if (!REF) { lvl = max(choose_level(t, fp, prand(px, t.id ^ salt)), 1u); }
          let bh = bark_hit(t, o, L, 0.0, REF && in_crown, c);
          if (bh.x < t1) { return 0.0; }
          if (!in_crown) { continue; }
          if (lvl == 0u) {
            if (leaf_walk(t, o, L, ta, civ.y, INF, cap_leaf_cells(), c).t < INF) { return 0.0; }
          } else if (lvl == 1u) {
            T *= vol_trans(t, o, L, ta, civ.y, fp, F.lod2.w, prand(px, t.id ^ salt ^ 0x9e37u), c);
          } else {
            let fv = far_iv(t, o, L);
            let fa = max(fv.x, 0.0);
            if (fv.y > fa) { T *= exp(-far_sigma(t.kind, abs(L.y)) * (fv.y - fa)); }
          }
          if (T < 0.01) { return 0.0; }
        }
      }
    }
    w.i += w.di;
  }
  (*lc).caps += 1u;
  return T;
}

// ---- sky occlusion (a cheap stand-in for spike 05's question) ---------------------------------------------------------

/** Openness of the sky above p: the nine nearest crowns as spheres (cosine-weighted solid angle),
 *  times a term for the rest of the forest blocking the low sky. */
fn sky_open(p: vec3f, n: vec3f, gy: f32, lc: ptr<function, LC>) -> f32 {
  let ci = vec2i(floor(p.xz / TREE_C));
  var vis = 1.0;
  for (var dz = -1; dz <= 1; dz++) {
    for (var dx = -1; dx <= 1; dx++) {
      (*lc).ao += 1u;
      let cj = ci + vec2i(dx, dz);
      let tl = tree_lite(cj, 0u);
      if (!tl.exists) { continue; }
      let t = tree_full(tl, cj, 0u);
      let cc = vec3f(t.pos.x, gy + t.cy + select(0.0, t.rv * 0.3, t.kind == 1u), t.pos.y);
      let R = select(t.rh * 0.85, t.rh, t.kind == 0u);
      let v = cc - p;
      let D2 = dot(v, v);
      let occ = clamp(R * R / D2, 0.0, 1.0) * clamp(dot(n, v) * inverseSqrt(D2) * 0.8 + 0.2, 0.0, 1.0);
      vis *= 1.0 - 0.8 * occ;
    }
  }
  return vis * (1.0 - 0.35 * forest_mask(p.x, p.z) * F.world.x);
}

// ---- shading ------------------------------------------------------------------------------------------------------------

fn sky_col(d: vec3f) -> vec3f {
  let y = max(d.y, 0.0);
  var c = mix(vec3f(0.66, 0.74, 0.82), vec3f(0.18, 0.34, 0.68), pow(y, 0.5));
  let sd = max(dot(d, F.sun.xyz), 0.0);
  c += vec3f(1.0, 0.82, 0.55) * (0.22 * pow(sd, 6.0) + 0.9 * pow(sd, 120.0));
  if (sd > cos(F.sun.w)) { c += vec3f(60.0, 55.0, 45.0); }
  return c;
}

fn sky_amb(n: vec3f) -> vec3f {
  return mix(vec3f(0.30, 0.33, 0.30), vec3f(0.42, 0.55, 0.78), 0.5 + 0.5 * n.y) * 0.75;
}
const BOUNCE = vec3f(0.10, 0.11, 0.055);

fn fog(col: vec3f, t: f32, d: vec3f) -> vec3f {
  let f = max(1.0 - exp(-t * 0.0004), smoothstep(0.7 * F.jit.z, F.jit.z, t));
  let sd = max(dot(d, F.sun.xyz), 0.0);
  let fc = mix(vec3f(0.60, 0.68, 0.77), vec3f(0.95, 0.85, 0.66), 0.5 * pow(sd, 5.0));
  return mix(col, fc, f);
}

fn tonemap(x: vec3f) -> vec3f {
  let c = x * EXPOSURE;
  return clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), vec3f(0.0), vec3f(1.0));
}

/** Henyey–Greenstein, scaled so the isotropic case is 1. */
fn hg(c: f32, g: f32) -> f32 { return (1.0 - g * g) / pow(1.0 + g * g - 2.0 * g * c, 1.5); }

fn shade_surface(mat: u32, alb: vec3f, n: vec3f, d: vec3f, trans: f32, V: f32, skyv: f32, ao: f32) -> vec3f {
  let L = F.sun.xyz;
  let ndl = dot(n, L);
  let amb = alb * (sky_amb(n) * skyv + BOUNCE) * ao;
  if (mat == MAT_GROUND || mat == MAT_BARK) {
    return SUN_C * V * alb * max(ndl, 0.0) + amb;
  }
  // Thin leaves, blades and fronds: two-sided, n faces the viewer.
  let front = max(ndl, 0.0);
  let back = max(-ndl, 0.0);
  let tcol = alb * vec3f(1.45, 1.5, 0.55);
  let fwd = pow(max(dot(d, L), 0.0), 3.0);
  let h = normalize(L - d);
  let spec = 0.06 * pow(max(dot(n, h), 0.0), 48.0) * front;
  let direct = alb * front + tcol * trans * back * (0.9 + 2.0 * fwd) + vec3f(spec);
  return SUN_C * V * direct + amb;
}

/** Grass beyond the explicit blades: the blades' thin model (shade_surface) averaged over eight
 *  azimuths, with their mean visible colour and occlusion; `cov` of it over the ground. */
fn shade_grass_cover(soil: vec3f, n: vec3f, d: vec3f, cov: f32, V: f32, skyv: f32, ao: f32) -> vec3f {
  let ground = shade_surface(MAT_GROUND, soil, n, d, 0.0, V, skyv, ao);
  var acc = vec3f(0.0);
  for (var k = 0u; k < 8u; k++) {
    let a = f32(k) * 0.78539816;
    var nb = vec3f(cos(a), 0.0, sin(a));
    if (dot(nb, d) > 0.0) { nb = -nb; }
    acc += shade_surface(MAT_GRASS, vec3f(0.145, 0.173, 0.041), normalize(nb + vec3f(0.0, 0.15, 0.0)), d, 0.55, V, skyv, 0.75 * ao);
  }
  return mix(ground, acc / 8.0, cov);
}

/** A stratified sample of a species' leaf normals (the tile's distribution, world.js), k = 0…7. */
fn leaf_normal(kind: u32, k: u32) -> vec3f {
  let a = select(0.25, 0.75, (k & 1u) == 1u);
  if (kind == 0u) {
    let b = select(0.25, 0.75, (k & 2u) == 2u);
    let c = select(0.25, 0.75, (k & 4u) == 4u);
    return normalize(vec3f((a - 0.5) * 2.4, 0.55 + 0.9 * b, (c - 0.5) * 2.4));
  }
  let c = 0.125 + 0.25 * f32(k >> 1u);
  return normalize(vec3f((a - 0.5) * 1.4, 1.0, (c - 0.5) * 1.4));
}

/** The foliage volume: the same two-sided thin-leaf model the explicit leaves use (shade_surface),
 *  averaged over the species' leaf orientations, so a crown keeps its look as it turns into volume.
 *  The volume's shape comes from its sun ray and its crown AO, as the explicit leaves' does. */
fn shade_volume(alb: vec3f, conif: f32, d: vec3f, V: f32, skyv: f32, ao: f32) -> vec3f {
  var acc = vec3f(0.0);
  var wtot = 0.0;
  for (var kind = 0u; kind < 2u; kind++) {
    let w = select(1.0 - conif, conif, kind == 1u);
    if (w < 0.02) { continue; }
    let trans = select(0.55, 0.25, kind == 1u);
    for (var k = 0u; k < 8u; k++) {
      var n = leaf_normal(kind, k);
      if (dot(n, d) > 0.0) { n = -n; }
      acc += w * shade_surface(MAT_LEAF, alb, n, d, trans, V, skyv, ao);
      wtot += w;
    }
  }
  return acc / max(wtot, 1e-4);
}

fn heat(x: f32) -> vec3f {
  let t = clamp(x, 0.0, 1.0);
  return clamp(vec3f(1.5 * t, 1.5 * t * t, 0.5 - t) + vec3f(0.0, 0.0, 0.2) * (1.0 - t), vec3f(0.0), vec3f(1.0));
}

var<workgroup> wsum: array<atomic<u32>, 16>;

@compute @workgroup_size(8, 8)
fn light(@builtin(global_invocation_id) gid: vec3u, @builtin(local_invocation_index) li: u32) {
  var c: Counters;
  var lc: LC;
  var two = 0u;
  let tp = tile_px(gid);
  let px = tp.xy;
  let valid = tp.z == 1u;
  if (valid) {
    let a = textureLoad(gA, vec2i(px), 0);
    let b = textureLoad(gB, vec2i(px), 0);
    let top = bitcast<f32>(a.x);
    let tfa = unpack2x16float(a.y);
    let oal = alb_dec(a.z);
    let fal = alb_dec(a.w);
    let mat = u32(round(oal.w * 255.0));
    let nop = oct_dec(b.x & 0xFFFFFFu);
    let aop = f32(b.x >> 24u) / 255.0;
    let conif = f32(b.y & 255u) / 255.0;
    let trans = f32(b.y >> 24u) / 255.0;
    let af = tfa.y;

    var jit = F.jit.xy;
    if (REF) { jit = vec2f(prand(px, 0x1234u), prand(px, 0x5678u)) - 0.5; }
    let d = ray_dir(vec2f(px) + 0.5 + jit);
    let o = F.eye.xyz;
    let pf = F.eye.w;

    var col = vec3f(0.0);
    if (mat == MAT_DEBUG) {
      col = oal.rgb;
    } else {
      var cop = vec3f(0.0);
      if (af < 0.995) {
        if (top >= 0.5 * INF) {
          cop = sky_col(d);
        } else {
          let p = o + d * top;
          let thin = mat == MAT_LEAF || mat == MAT_NEEDLE || mat == MAT_GRASS || mat == MAT_FERN;
          var V = 1.0;
          if (SHADOWS && (thin || trans > 0.0 || dot(nop, F.sun.xyz) > 0.0)) {
            V = sun_vis(p + nop * 0.01, pf * top, px, 0x51u, &c, &lc);
          }
          var skyv = 1.0;
          if (!dbg(DBG_AO) && (mat == MAT_GROUND || mat == MAT_GRASS || mat == MAT_FERN || mat == MAT_BARK)) {
            let gy = select(p.y, terrain_h(p.x, p.z), mat == MAT_BARK);
            skyv = sky_open(p, nop, gy, &lc);
          } else {
            skyv = 1.0 - 0.3 * forest_mask(p.x, p.z) * F.world.x;
          }
          if (mat == MAT_GROUND && trans > 0.0) { cop = fog(shade_grass_cover(oal.rgb, nop, d, trans, V, skyv, aop), top, d); }
          else { cop = fog(shade_surface(mat, oal.rgb, nop, d, trans, V, skyv, aop), top, d); }
        }
      }
      var cf = vec3f(0.0);
      if (af > 0.005) {
        let p = o + d * tfa.x;
        var V = 1.0;
        if (SHADOWS) { V = sun_vis(p, pf * tfa.x, px, 0xA7u, &c, &lc); }
        let skyv = 1.0 - 0.3 * forest_mask(p.x, p.z) * F.world.x;
        cf = fog(shade_volume(fal.rgb, conif, d, V, skyv, fal.w), tfa.x, d);
        if (af < 0.995) { two = 1u; }
      }
      col = tonemap(cf * af + cop * (1.0 - af));
      if (VIEW == 2u) {
        col = heat(f32(lc.slabs + lc.cells + 3u * lc.trees + c.leaf_cells + c.leaf_tests / 2u + 2u * c.vol + c.trunk + c.branch + lc.ao) / 120.0);
      }
    }
    textureStore(out_tex, vec2i(px), vec4f(col, 1.0));
  }

  if (STATS) {
    if (li < 16u) { atomicStore(&wsum[li], 0u); }
    workgroupBarrier();
    if (valid) {
      atomicAdd(&wsum[0], lc.rays);
      atomicAdd(&wsum[1], lc.slabs);
      atomicAdd(&wsum[2], lc.cells);
      atomicAdd(&wsum[3], lc.trees);
      atomicAdd(&wsum[4], c.vol);
      atomicAdd(&wsum[5], c.leaf_cells);
      atomicAdd(&wsum[6], c.leaf_tests);
      atomicAdd(&wsum[7], lc.caps + c.cap_l + c.cap_v);
      atomicAdd(&wsum[8], lc.ao);
      atomicAdd(&wsum[9], two);
      atomicAdd(&wsum[10], c.trunk + c.branch);
      if (lc.caps + c.cap_l + c.cap_v > 0u) { atomicAdd(&wsum[11], 1u); }
    }
    workgroupBarrier();
    if (li < 16u) { atomicAdd(&stats[32u + li], atomicLoad(&wsum[li])); }
  }
}
