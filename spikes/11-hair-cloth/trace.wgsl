// The marched frame in one compute kernel: ground, tower, bodies, cloth (a field or its triangles),
// hair (a density field or strands), shadows and shading. Appended to common.wgsl + world.wgsl.

@group(0) @binding(0) var<uniform> F: Frame;
@group(0) @binding(1) var<uniform> M: Mats;
@group(0) @binding(2) var<storage, read> prims: array<vec4f>;
@group(0) @binding(3) var<storage, read> chars: array<vec4f>;
@group(0) @binding(4) var<storage, read> pos: array<vec4f>;
@group(0) @binding(5) var<storage, read> attr: array<vec4f>;
@group(0) @binding(6) var<storage, read> segs: array<vec4f>;
@group(0) @binding(7) var<storage, read> sheetb: array<u32>;
@group(0) @binding(8) var hvol: texture_3d<f32>;
@group(0) @binding(9) var hlight: texture_3d<f32>;
@group(0) @binding(10) var hskip: texture_3d<f32>;
@group(0) @binding(11) var lsamp: sampler;
@group(0) @binding(12) var smap: texture_depth_2d;
@group(0) @binding(13) var csamp: sampler_comparison;
@group(1) @binding(0) var out_tex: texture_storage_2d<rgba16float, write>;
@group(1) @binding(1) var depth_tex: texture_storage_2d<r32float, write>;
@group(1) @binding(2) var<storage, read_write> stats: array<atomic<u32>, 64>;
@group(1) @binding(3) var<storage, read> boff: array<u32>;
@group(1) @binding(4) var<storage, read> blist: array<u32>;
@group(1) @binding(5) var zbuf: texture_depth_2d;

// Variants: each combination compiles to its own specialized kernel.
override CLOTH: u32 = 0u;          // 0 none, 1 sphere-traced patch field, 2 analytic triangles
override HAIR: u32 = 0u;           // 0 none, 1 strands, 2 density volume
override HAIRSH: bool = false;     // hair casts shadows (from its density grid)
override CLOTHSH: bool = true;     // cloth casts shadows (switch, to measure their cost)
override FIELD_SOFT: bool = false; // field cloth casts soft-estimate shadows instead of hard ones (to measure)
override SMAP: bool = false;       // raster fallback: cloth shadows from the shadow map
override REF: bool = false;        // brute-force reference: no binning, small steps, many layers
override REF_LIGHT: bool = false;  // hair lit by an exact per-sample march, not the light grid
override STATS: bool = false;
override VIEW: u32 = 0u;           // 1 cloth-step heat map, 2 strand-candidate heat map
override DEPTH_OUT: bool = false;  // write marched depth for the raster fallback
override RASTER_BOUND: bool = false; // raster fallback: the meshes' depth bounds every march
override P: u32 = 1u;              // quads per patch side
override STEP_SCALE: f32 = 1.0;    // < 1 for the reference
override EPS_SCALE: f32 = 0.25;    // hit tolerance, in pixel footprints
override MAX_STEPS: u32 = 96u;
override SHADOW_STEPS: u32 = 48u;
override LAYERS: u32 = 4u;         // strand layers kept per pixel
override VOL_STEP: f32 = 0.5;      // hair volume step, in voxels

const LMAX: u32 = 16u;
// Safety caps per pixel, so no list or march can run away (spikes/README "GPU safety rules").
// Hitting one counts as a cap hit in the stats, so it shows up in the correctness check.
const MAX_LIST: u32 = 4096u;           // candidates read from one tile list
const CLOTH_BUDGET: u32 = 1536u;       // total cloth march steps per pixel
const VOL_ITERS: u32 = 768u;           // hair volume samples per pixel

// Stats slots (main.js STAT_NAMES).
const S_CLOTH_PX: u32 = 0u;
const S_CHAR_PX: u32 = 1u;
const S_GROUND_PX: u32 = 2u;
const S_TOWER_PX: u32 = 3u;
const S_SKY_PX: u32 = 4u;
const S_HAIR_PX: u32 = 5u;
const S_C_MARCHES: u32 = 6u;
const S_C_STEPS: u32 = 7u;
const S_C_HITS: u32 = 8u;
const S_C_MISSES: u32 = 9u;
const S_C_CAPS: u32 = 10u;
const S_C_CANDS: u32 = 11u;
const S_C_ENTERED: u32 = 12u;
const S_C_TRIS: u32 = 13u;
const S_HIST: u32 = 14u;          // 5 buckets: < 1 mm, < 4 mm, < 16 mm, < 64 mm, ≥ 64 mm
const S_MINSTEP: u32 = 19u;
const S_B_MARCHES: u32 = 20u;
const S_B_STEPS: u32 = 21u;
const S_B_CAPS: u32 = 22u;
const S_S_CANDS: u32 = 23u;
const S_S_LAYERS: u32 = 24u;
const S_S_PX: u32 = 25u;
const S_V_SAMPLES: u32 = 26u;
const S_V_PX: u32 = 27u;
const S_SH_RAYS: u32 = 28u;
const S_SH_STEPS: u32 = 29u;
const S_SH_CAPS: u32 = 30u;
const S_SH_HAIR: u32 = 31u;
const S_CLOTH_PX_STEPS: u32 = 32u;
const S_MAX_C_STEPS: u32 = 33u;

fn srange(tile: u32, cat: u32) -> vec2u {
  let i = tile * NCAT + cat;
  return vec2u(boff[i], boff[i + 1u]);
}

// ---- cloth -------------------------------------------------------------------------------------------

fn hist(h: f32) {
  if (!STATS) { return; }
  var b = 4u;
  if (h < 0.001) { b = 0u; } else if (h < 0.004) { b = 1u; } else if (h < 0.016) { b = 2u; } else if (h < 0.064) { b = 3u; }
  switch b {
    case 0u: { cnt.h0 += 1u; }
    case 1u: { cnt.h1 += 1u; }
    case 2u: { cnt.h2 += 1u; }
    case 3u: { cnt.h3 += 1u; }
    default: { cnt.h4 += 1u; }
  }
  cnt.minstep = min(cnt.minstep, h);
}

/** Tests one patch against the ray; updates the nearest hit. */
fn cloth_candidate(pi: u32, o: vec3f, d: vec3f, inv: vec3f, hit: ptr<function, ClothHit>) {
  cnt.c_cands += 1u;
  let iv = patch_interval(pi, o, d, inv);
  if (iv.x > iv.y || iv.x >= (*hit).t) { return; }
  cnt.c_entered += 1u;
  let inf = patch_info(pi);
  if (CLOTH == 2u) {
    for (var k = 0u; k < 2u * P * P; k++) {
      let tl = tri_local(k);
      let a = pos[patch_vertex(inf, tl.x % (P + 1u), tl.x / (P + 1u))].xyz;
      let b = pos[patch_vertex(inf, tl.y % (P + 1u), tl.y / (P + 1u))].xyz;
      let c = pos[patch_vertex(inf, tl.z % (P + 1u), tl.z / (P + 1u))].xyz;
      cnt.c_tris += 1u;
      let r = ray_tri(o, d, a, b, c);
      if (r.x < (*hit).t) { *hit = ClothHit(r.x, pi, k, vec3f(1.0 - r.y - r.z, r.y, r.z)); }
    }
    return;
  }
  // Sphere tracing on the exact distance to the patch's triangles, inside its bound.
  cnt.c_marches += 1u;
  var t = max(iv.x, 0.0);
  let t1 = min(iv.y, (*hit).t);
  for (var i = 0u; i < MAX_STEPS; i++) {
    if (!REF && cnt.c_steps >= CLOTH_BUDGET) { break; }
    cnt.c_steps += 1u;
    let p = o + d * t;
    let eps = max(EPS_SCALE * t * F.eye.w, 1e-4);
    let h = patch_d(inf, p);
    if (h < eps) {
      cnt.c_hits += 1u;
      *hit = ClothHit(t, pi, 0xffffffffu, vec3f(0.0));
      return;
    }
    hist(h);
    t += h * STEP_SCALE;
    if (t > t1) { cnt.c_misses += 1u; return; }
  }
  cnt.c_caps += 1u;
}

/** Nearest triangle of patch pi to p, and the closest point's barycentrics on it. */
fn patch_closest(pi: u32, p: vec3f) -> vec4f {
  let inf = patch_info(pi);
  var best = BIG;
  var bk = 0u;
  for (var k = 0u; k < 2u * P * P; k++) {
    let tl = tri_local(k);
    let a = pos[patch_vertex(inf, tl.x % (P + 1u), tl.x / (P + 1u))].xyz;
    let b = pos[patch_vertex(inf, tl.y % (P + 1u), tl.y / (P + 1u))].xyz;
    let c = pos[patch_vertex(inf, tl.z % (P + 1u), tl.z / (P + 1u))].xyz;
    let dd = ud_tri(p, a, b, c);
    if (dd < best) { best = dd; bk = k; }
  }
  let tl = tri_local(bk);
  let a = pos[patch_vertex(inf, tl.x % (P + 1u), tl.x / (P + 1u))].xyz;
  let b = pos[patch_vertex(inf, tl.y % (P + 1u), tl.y / (P + 1u))].xyz;
  let c = pos[patch_vertex(inf, tl.z % (P + 1u), tl.z / (P + 1u))].xyz;
  return vec4f(closest_bary(p, a, b, c), bitcast<f32>(bk));
}

// ---- bodies ------------------------------------------------------------------------------------------

struct CharHit { t: f32, c: u32, m: u32 }

fn char_candidate(c: u32, o: vec3f, d: vec3f, inv: vec3f, hit: ptr<function, CharHit>, tmax: f32) {
  let lo = chars[c * CS + C_LO].xyz;
  let hi = chars[c * CS + C_HI].xyz;
  let bx = ray_box(o, inv, lo, hi);
  let lim = min(tmax, (*hit).t);
  if (bx.x > bx.y || bx.x >= lim) { return; }
  let iv = char_mask(c, o, d, lim);
  if (iv.m == 0u) { return; }
  cnt.b_marches += 1u;
  var t = max(iv.t0, 0.0);
  let t1 = min(iv.t1, lim);
  for (var i = 0u; i < MAX_STEPS; i++) {
    cnt.b_steps += 1u;
    let h = char_d(c, iv.m, o + d * t);
    if (h < max(EPS_SCALE * t * F.eye.w, 1e-4)) {
      *hit = CharHit(t + h, c, iv.m);
      return;
    }
    t += h * STEP_SCALE;
    if (t > t1) { return; }
  }
  cnt.b_caps += 1u;
}

// ---- hair ------------------------------------------------------------------------------------------------

fn hair_base(p: vec3f, var_: f32) -> vec3f {
  let h = chars[C_HAIR].rgb;
  return h * (0.75 + 0.5 * var_);
}

/** Strand-scale detail for the density field: noise stretched along the flow, faded by footprint. */
fn strand_detail(p: vec3f, tg: vec3f, fp: f32) -> f32 {
  let w = 1.0 - smoothstep(0.0007, 0.0022, fp);
  if (w <= 0.0) { return 1.0; }
  let along = dot(p, tg);
  let q = p - tg * along;
  let n = noise3(q * 650.0 + tg * along * 5.0) * 0.6 + noise3(q * 1500.0 + tg * along * 9.0) * 0.4;
  return mix(1.0, clamp(0.15 + 2.2 * max(n + 0.15, 0.0), 0.0, 2.5), w);
}


fn coverage(dist: f32, r: f32, fpr: f32) -> f32 {
  let lo = max(dist - r, -fpr);
  let hi = min(dist + r, fpr);
  return clamp((hi - lo) / (2.0 * fpr), 0.0, 1.0);
}

// ---- the kernel -----------------------------------------------------------------------------------------

fn heat(x: f32) -> vec3f {
  let t = clamp(x, 0.0, 1.0);
  return clamp(vec3f(1.5 * t, 1.5 * t * t, 0.5 - t) + vec3f(0.0, 0.0, 0.2) * (1.0 - t), vec3f(0.0), vec3f(1.0));
}

@compute @workgroup_size(8, 8)
fn trace(@builtin(global_invocation_id) gid: vec3u) {
  let px = vec2u(gid.x + F.hair.w, gid.y + F.misc.w);
  if (px.x >= F.dims.x || px.y >= F.dims.y) { return; }
  cnt.minstep = 1e9;
  let tile = (px.y / 8u) * F.dims.z + px.x / 8u;
  let su = (f32(px.x) + 0.5) / f32(F.dims.x) * 2.0 - 1.0;
  let sv = 1.0 - (f32(px.y) + 0.5) / f32(F.dims.y) * 2.0;
  let o = F.eye.xyz;
  let d = normalize(F.cf.xyz + F.cr.xyz * su + F.cu.xyz * sv);
  let inv = vec3f(safe_inv(d.x), safe_inv(d.y), safe_inv(d.z));

  // Raster fallback: the meshes were rasterized first; nothing behind them is marched or shaded.
  var t_raster = INF;
  if (RASTER_BOUND) {
    let z = textureLoad(zbuf, vec2i(px), 0);
    if (z < 1.0) { t_raster = (NEAR_Z * FAR_Z / (FAR_Z - NEAR_Z)) / (FAR_Z / (FAR_Z - NEAR_Z) - z) / dot(d, F.cf.xyz); }
  }

  // Ground (a plane) and tower.
  var best = min(select(INF, -o.y / d.y, d.y < -1e-6), t_raster);
  var kind = select(0u, 1u, best < t_raster);
  let tt = tower_trace(o, d, best, EPS_SCALE, select(160u, 4000u, REF));
  if (tt < best) { best = tt; kind = 2u; }

  // Cloth before bodies: robes cover bodies, so the body march is cut short by the cloth hit.
  var ch = ClothHit(best, 0u, 0u, vec3f(0.0));
  if (CLOTH > 0u) {
    if (REF) {
      for (var s = 0u; s < F.misc.z; s++) {
        let bx = sheet_box(s);
        let iv = ray_box(o, inv, bx[0] - CLOTH_H, bx[1] + CLOTH_H);
        if (iv.x > iv.y || iv.x >= ch.t) { continue; }
        let p0 = u32(M.m[s * 4u + 3u].x);
        let pn = u32(M.m[s * 4u + 3u].y);
        for (var pi = p0; pi < p0 + pn; pi++) { cloth_candidate(pi, o, d, inv, &ch); }
      }
    } else {
      let r = srange(tile, CAT_PATCH);
      for (var e = r.x; e < min(r.y, r.x + MAX_LIST); e++) { cloth_candidate(blist[e], o, d, inv, &ch); }
    }
    if (ch.t < best) { best = ch.t; kind = 4u; }
  }

  var bh = CharHit(best, 0u, 0u);
  if (REF) {
    for (var c = 0u; c < F.grid.y; c++) { char_candidate(c, o, d, inv, &bh, best); }
  } else {
    let r = srange(tile, CAT_CHAR);
    for (var e = r.x; e < min(r.y, r.x + MAX_LIST); e++) { char_candidate(blist[e], o, d, inv, &bh, best); }
  }
  if (bh.t < best) { best = bh.t; kind = 3u; }

  // Shade the opaque hit.
  let p = o + d * best;
  let fp = best * F.eye.w;
  let eps = EPS_SCALE * fp;
  var color = sky(d);
  if (kind == 1u) {
    let a = ground_albedo(p, fp);
    let n = vec3f(0.0, 1.0, 0.0);
    color = shade(a.rgb, a.w, n, -d, sun_vis(p, n, eps, true), 1.0, 0.0);
  } else if (kind == 2u) {
    let n = tower_normal(p);
    let a = tower_albedo(p, n, fp);
    color = shade(a.rgb, a.w, n, -d, sun_vis(p, n, eps, true), 1.0, 0.0);
  } else if (kind == 3u) {
    let n = char_normal(bh.c, bh.m, p);
    let a = char_material(bh.c, bh.m, p);
    color = shade(a.rgb, a.w, n, -d, sun_vis(p, n, eps, true), 1.0, 0.3);
  } else if (kind == 4u) {
    var k = ch.tri;
    var bary = ch.bary;
    if (CLOTH == 1u) {
      let cb = patch_closest(ch.pid, p);
      k = bitcast<u32>(cb.w);
      bary = cb.xyz;
    }
    let at = cloth_attr(ch.pid, k, bary);
    let nrm = at[0].xyz;
    let sunside = select(-nrm, nrm, dot(nrm, F.sun.xyz) > 0.0);
    let vis = sun_vis(p, sunside, eps + CLOTH_H, true);
    color = shade_cloth(p, d, nrm, at[1].xy, u32(at[1].z), fp, vis, vis);
  }
  if (kind != 0u) { color = fog(color, best, d); }
  let raster_owned = RASTER_BOUND && kind == 0u && t_raster < INF;

  // Hair, composited over the opaque hit.
  var hair_px = false;
  if (HAIR == 2u) {
    let g = hair_grid();
    let vx = g.w;
    let iv = ray_box(o, inv, g.xyz, g.xyz + vec3f(hd()) * vx);
    let t1 = min(iv.y, best);
    if (iv.x <= iv.y && vx > 0.0 && max(iv.x, 0.0) < t1) {
      let dt = VOL_STEP * vx;
      var t = max(iv.x, 0.0) + dt * hash_f(px.x * 1973u + px.y * 9277u + 1u) * select(1.0, 0.0, REF);
      var T = 1.0;
      var C = vec3f(0.0);

      for (var i = 0u; i < select(VOL_ITERS, 4096u, REF); i++) {
        if (t > t1) { break; }
        let q = o + d * t;
        let x = (q - g.xyz) / vx;
        if (!REF) {
          let sk = textureLoad(hskip, vec3i(x / f32(HCOARSE)), 0).x;
          if (sk > 0.0) { t += sk; continue; }
        }
        cnt.v_samples += 1u;
        let s = textureSampleLevel(hvol, lsamp, x / vec3f(hd()), 0.0);
        if (s.x > 0.05) {
          let tg = normalize(s.yzw + vec3f(0.0, -1e-4, 0.0));
          let sigma = s.x * strand_detail(q, tg, t * F.eye.w);
          let a = 1.0 - exp(-sigma * dt);
          hair_px = true;
          let lt = hair_light_at(q);
          let base = hair_base(q, noise3(q * 30.0) * 0.5 + 0.5);
          C += T * a * hair_shade(tg, -d, base, lt, 0.25 + 0.75 * lt);
          T *= 1.0 - a;
          if (T < select(0.004, 0.0001, REF)) { T = 0.0; break; }
        }
        t += dt;
      }
      color = C + T * color;
    }
  } else if (HAIR == 1u) {
    var lt: array<f32, LMAX>;
    var la: array<f32, LMAX>;
    var li: array<u32, LMAX>;
    var ls: array<f32, LMAX>;
    var nl = 0u;
    var tail = 1.0;                    // transmittance of the layers that weren't kept: exact, as a product
    let sps = F.misc.y;
    let fps = 0.5 * F.eye.w;
    var lo = 0u;
    var hi = 0u;
    if (REF) { hi = F.misc.x; } else { let r = srange(tile, CAT_SEG); lo = r.x; hi = min(r.y, r.x + MAX_LIST); }
    for (var e = lo; e < hi; e++) {
      var j0 = 0u;
      var j1 = 0u;
      if (REF) {
        let a = prims[pr_strand(e)];
        let b = prims[pr_strand(e) + 1u];
        let iv = ray_box(o, inv, a.xyz, b.xyz);
        if (iv.x > iv.y || iv.x >= best) { continue; }
        j0 = e * sps;
        j1 = j0 + sps;
      } else {
        j0 = blist[e];
        j1 = j0 + 1u;
      }
      for (var j = j0; j < j1; j++) {
        cnt.s_cands += 1u;
        let a = segs[2u * j];
        let b = segs[2u * j + 1u];
        if (max(a.w, b.w) <= 0.0) { continue; }
        let q = ray_segment(o, d, a.xyz, b.xyz);
        if (q.y >= best || q.y <= 0.0) { continue; }
        if (q.z <= 0.0 && (j % sps) != 0u) { continue; }   // the previous segment owns the joint
        let r = mix(a.w, b.w, q.z);
        let fpr = max(q.y * fps, 1e-5);
        if (q.x >= r + fpr) { continue; }
        let al = coverage(q.x, r, fpr) * 0.95;
        if (al <= 0.002) { continue; }
        // Keep the LAYERS nearest, sorted by t.
        if (nl == LAYERS && q.y >= lt[LAYERS - 1u]) { tail *= 1.0 - al; continue; }
        if (nl == LAYERS) { tail *= 1.0 - la[LAYERS - 1u]; }
        var k = min(nl, LAYERS - 1u);
        if (nl < LAYERS) { nl += 1u; }
        while (k > 0u && lt[k - 1u] > q.y) {
          lt[k] = lt[k - 1u]; la[k] = la[k - 1u]; li[k] = li[k - 1u]; ls[k] = ls[k - 1u];
          k -= 1u;
        }
        lt[k] = q.y; la[k] = al; li[k] = j; ls[k] = q.z;
      }
    }
    if (nl > 0u) {
      hair_px = true;
      cnt.s_layers += nl;
      var T = 1.0;
      var C = vec3f(0.0);
      var last = vec3f(0.0);
      for (var k = 0u; k < nl; k++) {
        let j = li[k];
        let a = segs[2u * j];
        let b = segs[2u * j + 1u];
        let tg = normalize(b.xyz - a.xyz);
        let q = o + d * lt[k];
        let lt_ = hair_light_at(q);
        let strand = j / sps;
        let base = hair_base(q, hash_f(strand * 7919u + 13u));
        last = hair_shade(tg, -d, base, lt_, 0.25 + 0.75 * lt_);
        C += T * la[k] * last;
        T *= 1.0 - la[k];
      }
      // Layers beyond the kept ones: their coverage is exact, their colour is the last layer's.
      C += T * (1.0 - tail) * last * 0.8;
      T *= tail;
      color = C + T * color;
    }
  }

  var outc = tonemap(color);
  if (VIEW == 1u && cnt.c_marches > 0u) { outc = heat(f32(cnt.c_steps) / 64.0); }
  if (VIEW == 1u && cnt.c_marches == 0u) { outc = outc * 0.25; }
  if (VIEW == 2u) { outc = select(outc * 0.25, heat(f32(cnt.s_cands) / 512.0), cnt.s_cands > 0u); }
  if (!raster_owned) { textureStore(out_tex, vec2i(px), vec4f(outc, 1.0)); }
  if (DEPTH_OUT) { textureStore(depth_tex, vec2i(px), vec4f(select(best, 1e9, best >= INF || raster_owned), 0.0, 0.0, 0.0)); }

  if (STATS) {
    switch kind {
      case 0u: { atomicAdd(&stats[S_SKY_PX], 1u); }
      case 1u: { atomicAdd(&stats[S_GROUND_PX], 1u); }
      case 2u: { atomicAdd(&stats[S_TOWER_PX], 1u); }
      case 3u: { atomicAdd(&stats[S_CHAR_PX], 1u); }
      default: { atomicAdd(&stats[S_CLOTH_PX], 1u); atomicAdd(&stats[S_CLOTH_PX_STEPS], cnt.c_steps); }
    }
    if (hair_px) { atomicAdd(&stats[S_HAIR_PX], 1u); }
    atomicAdd(&stats[S_C_MARCHES], cnt.c_marches);
    atomicAdd(&stats[S_C_STEPS], cnt.c_steps);
    atomicAdd(&stats[S_C_HITS], cnt.c_hits);
    atomicAdd(&stats[S_C_MISSES], cnt.c_misses);
    atomicAdd(&stats[S_C_CAPS], cnt.c_caps);
    atomicAdd(&stats[S_C_CANDS], cnt.c_cands);
    atomicAdd(&stats[S_C_ENTERED], cnt.c_entered);
    atomicAdd(&stats[S_C_TRIS], cnt.c_tris);
    atomicAdd(&stats[S_HIST], cnt.h0);
    atomicAdd(&stats[S_HIST + 1u], cnt.h1);
    atomicAdd(&stats[S_HIST + 2u], cnt.h2);
    atomicAdd(&stats[S_HIST + 3u], cnt.h3);
    atomicAdd(&stats[S_HIST + 4u], cnt.h4);
    if (cnt.minstep < 1e9) { atomicMin(&stats[S_MINSTEP], bitcast<u32>(cnt.minstep)); }
    atomicAdd(&stats[S_B_MARCHES], cnt.b_marches);
    atomicAdd(&stats[S_B_STEPS], cnt.b_steps);
    atomicAdd(&stats[S_B_CAPS], cnt.b_caps);
    atomicAdd(&stats[S_S_CANDS], cnt.s_cands);
    atomicAdd(&stats[S_S_LAYERS], cnt.s_layers);
    if (HAIR == 1u && hair_px) { atomicAdd(&stats[S_S_PX], 1u); }
    atomicAdd(&stats[S_V_SAMPLES], cnt.v_samples);
    if (HAIR == 2u && hair_px) { atomicAdd(&stats[S_V_PX], 1u); }
    atomicAdd(&stats[S_SH_RAYS], cnt.sh_rays);
    atomicAdd(&stats[S_SH_STEPS], cnt.sh_steps);
    atomicAdd(&stats[S_SH_CAPS], cnt.sh_caps);
    atomicAdd(&stats[S_SH_HAIR], cnt.sh_hair);
    atomicMax(&stats[S_MAX_C_STEPS], cnt.c_steps);
  }
}
