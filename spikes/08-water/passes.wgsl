// The six passes. Appended to world.wgsl + scene.wgsl + water.wgsl.
//
//   water_surface  full res   where each ray meets the water: the lake's mean plane, or the stream
//                             (marched); signed distance, negative = stream
//   scene_pass     full res   the opaque world, primary rays clipped at the water; the visible-water mask
//   water_gbuf     full res   for visible water: the hit refined on the wave field, the filtered normal
//                             and the slope variance filtering removed (a G-buffer, written once)
//   reflect_pass   1/rdiv     reflection rays
//   refract_pass   1/qdiv     refraction rays
//   composite      full res   Fresnel, upsampling, glint, foam, fog, tonemap
//
// The water slice is every pass except scene_pass. Each binding number is used by one variable
// only, so each pass's bind group layout lists just what it uses.

@group(1) @binding(0) var wt_w: texture_storage_2d<r32float, write>;
@group(1) @binding(1) var wt_r: texture_2d<f32>;
@group(1) @binding(2) var out_w: texture_storage_2d<rgba16float, write>;
@group(1) @binding(3) var wm_w: texture_storage_2d<r32float, write>;
@group(1) @binding(4) var wm_r: texture_2d<f32>;
@group(1) @binding(5) var rt_w: texture_storage_2d<rgba16float, write>;
@group(1) @binding(6) var rt_r: texture_2d<f32>;
@group(1) @binding(7) var qt_w: texture_storage_2d<rgba16float, write>;
@group(1) @binding(8) var qt_r: texture_2d<f32>;
@group(1) @binding(9) var wg_w: texture_storage_2d<rgba32float, write>;
@group(1) @binding(10) var wg_r: texture_2d<f32>;

const FAR: f32 = 420.0;
const REFR_LEN: f32 = 12.0;
const SIG_MAX: f32 = 0.4;      // slope deviations stored in the G-buffer are clamped to this

fn ray_dir(px: vec2f) -> vec3f {
  let u = px.x / f32(F.dims.x) * 2.0 - 1.0;
  let v = 1.0 - px.y / f32(F.dims.y) * 2.0;
  return normalize(F.cf.xyz + F.cr.xyz * u + F.cu.xyz * v);
}

fn pix(g: vec2u) -> vec2f { return vec2f(g) + 0.5 + F.misc.zw; }

/** This invocation's pixel in a pass at 1/dv resolution, offset into the current tile (F.tile, in
 *  full-resolution pixels); false if it falls outside the tile or the image. Brute-force references
 *  run tile by tile, one submission each, so no submission runs long (spikes/README.md). */
fn tile_px(gid: vec3u, dv: u32, g: ptr<function, vec2u>) -> bool {
  let lo = F.tile.xy / dv;
  let hi = min((F.tile.zw + dv - 1u) / dv, (F.dims.xy + dv - 1u) / dv);
  *g = gid.xy + lo;
  return all(*g < hi);
}

fn heat(x: f32) -> vec3f {
  let t = clamp(x, 0.0, 1.0);
  return clamp(vec3f(1.5 * t, 1.5 * t * t, 0.5 - t) + vec3f(0.0, 0.0, 0.2) * (1.0 - t), vec3f(0.0), vec3f(1.0));
}

fn lum(c: vec3f) -> f32 { return dot(c, vec3f(0.2126, 0.7152, 0.0722)); }

// ---- 1. water_surface ------------------------------------------------------------------------------------

@compute @workgroup_size(8, 8)
fn water_surface(@builtin(global_invocation_id) gid: vec3u) {
  var px: vec2u;
  if (!tile_px(gid, 1u, &px)) { return; }
  C.limit = MAX_STEPS;
  let d = ray_dir(pix(px));
  let w = water_hit(F.eye.xyz, d, F.eye.w, 0u);
  var v = 0.0;
  if (w.t < FAR) { v = select(w.t, -w.t, w.kind == 2u); }
  textureStore(wt_w, px, vec4f(v, 0.0, 0.0, 0.0));
  if (STATS) {
    atomicAdd(&stats[S_WS_STEPS], C.wsteps);
    atomicAdd(&stats[S_WS_CAPS], C.wcaps);
  }
}

// ---- 2. scene_pass ------------------------------------------------------------------------------------------

@compute @workgroup_size(8, 8)
fn scene_pass(@builtin(global_invocation_id) gid: vec3u) {
  var px: vec2u;
  if (!tile_px(gid, 1u, &px)) { return; }
  C.limit = MAX_STEPS;
  let o = F.eye.xyz;
  let d = ray_dir(pix(px));
  var tw = INF;
  var wv = 0.0;
  if (WATER) {
    wv = textureLoad(wt_r, px, 0).x;
    if (wv != 0.0) { tw = abs(wv); }
  }
  let cone = Cone(0.0, F.eye.w);
  let h = trace_world(o, d, 0.0, min(tw, FAR), cone);
  var col = vec3f(0.0);
  var mask = 0.0;
  if (h.kind != K_SKY) {
    let p = o + d * h.t;
    let s = surface(h, p, fp_at(cone, h.t));
    var sh = 1.0;
    // Foliage is displaced up to 0.37 m outside the base shape that casts shadows: start beyond it.
    let start = select(0.02, 0.45, s.transl > 0.0);
    if (dot(s.n, F.sun.xyz) > 0.0 || s.transl > 0.0) { sh = sun_vis(p, s.n, F.hor_sun.w, start); }
    col = fog(light(s, p, -d, sh), h.t, d);
  } else if (tw < INF) {
    mask = wv;
  } else {
    col = sky(d, true, F.eye.w);
  }
  if (mask == 0.0) {
    var outc = tonemap(col);
    if (VIEW == 3u) { outc = heat(f32(C.steps + C.tsteps) / 96.0); }
    textureStore(out_w, px, vec4f(outc, 1.0));
  }
  textureStore(wm_w, px, vec4f(mask, 0.0, 0.0, 0.0));
  if (STATS) {
    if (mask != 0.0) {
      atomicAdd(&stats[S_WATER_PX], 1u);
      if (mask < 0.0) { atomicAdd(&stats[S_STREAM_PX], 1u); }
    }
    if (h.kind == K_SKY && mask == 0.0) { atomicAdd(&stats[S_SKY_PX], 1u); }
    atomicAdd(&stats[S_SCENE_STEPS], C.steps);
    atomicAdd(&stats[S_SCENE_CAPS], C.caps);
    atomicAdd(&stats[S_SCENE_TSTEPS], C.tsteps);
    atomicAdd(&stats[S_SCENE_TCAPS], C.tcaps);
    atomicAdd(&stats[S_SCENE_CELLS], C.cells);
    atomicAdd(&stats[S_SHADOW_STEPS], C.shsteps);
  }
}

// ---- 3. water_gbuf -----------------------------------------------------------------------------------------

struct WGeo { t: f32, kind: u32, d: vec3f, n: vec3f, vpar: f32, vperp: f32 }

/** The water at a point, filtered for a footprint: normal and removed slope variance. */
fn water_geo(d: vec3f, t: f32, kind: u32, theta: f32) -> WGeo {
  let p = F.eye.xyz + d * t;
  let ws = water_at(p, kind, surface_fp(p, d, t, theta, kind), view_xz(d));
  return WGeo(t, kind, d, water_n(ws), ws.vpar, ws.vperp);
}

/** Packs two slope deviations into one float's bits. The 0.25 offset keeps the high half's
 *  exponent away from denormals and NaNs, so the bits survive a float texture. */
fn pack_sig(vpar: f32, vperp: f32) -> f32 {
  let s = vec2f(sqrt(vpar), sqrt(vperp));
  return bitcast<f32>(pack2x16unorm(0.25 + 0.5 * min(s / SIG_MAX, vec2f(1.0))));
}

fn unpack_var(x: f32) -> vec2f {
  let s = (unpack2x16unorm(bitcast<u32>(x)) - 0.25) * 2.0 * SIG_MAX;
  return s * s;
}

@compute @workgroup_size(8, 8)
fn water_gbuf(@builtin(global_invocation_id) gid: vec3u) {
  var px: vec2u;
  if (!tile_px(gid, 1u, &px)) { return; }
  let wv = textureLoad(wm_r, px, 0).x;
  if (wv == 0.0) { return; }
  C.limit = MAX_STEPS;
  let d = ray_dir(pix(px));
  let kind = select(1u, 2u, wv < 0.0);
  var t = abs(wv);
  if (kind == 1u && !REF) { t = lake_refine(F.eye.xyz, d, t, F.eye.w, NEWTON); }
  let g = water_geo(d, t, kind, F.eye.w);
  textureStore(wg_w, px, vec4f(select(t, -t, kind == 2u), g.n.x, g.n.z, pack_sig(g.vpar, g.vperp)));
  if (STATS) { atomicAdd(&stats[S_GB_STEPS], C.wsteps); }
}

fn read_geo(g: vec2u) -> WGeo {
  let s = textureLoad(wg_r, g, 0);
  let n = vec3f(s.y, sqrt(max(1.0 - s.y * s.y - s.z * s.z, 0.0)), s.z);
  let v = unpack_var(s.w);
  return WGeo(abs(s.x), select(1u, 2u, s.x < 0.0), ray_dir(pix(g)), n, v.x, v.y);
}

// ---- shared by the secondary-ray passes ------------------------------------------------------------------------

/** The water a reflection or refraction texel stands for. At full resolution, the pixel's own
 *  G-buffer entry. At 1/dv, if any pixel in the block is visible water: the ray through the block's
 *  centre, intersected and filtered at this pass's own (dv times wider) footprint, so its normal is
 *  bandlimited for its sample rate. If that ray misses the water, the first water pixel's ray. */
fn block_geo(g: vec2u, dv: u32, ok: ptr<function, bool>, fallback: ptr<function, bool>) -> WGeo {
  if (dv == 1u) {
    *ok = textureLoad(wm_r, g, 0).x != 0.0;
    if (*ok) { return read_geo(g); }
    return WGeo(0.0, 0u, vec3f(0.0), vec3f(0.0, 1.0, 0.0), 0.0, 0.0);
  }
  let base = g * dv;
  var first = vec2u(0u);
  var fw = 0.0;
  for (var j = 0u; j < dv; j++) {
    for (var i = 0u; i < dv; i++) {
      let q = base + vec2u(i, j);
      if (fw == 0.0 && q.x < F.dims.x && q.y < F.dims.y) {
        let w = textureLoad(wm_r, q, 0).x;
        if (w != 0.0) { first = q; fw = w; }
      }
    }
  }
  *ok = fw != 0.0;
  if (!*ok) { return WGeo(0.0, 0u, vec3f(0.0), vec3f(0.0, 1.0, 0.0), 0.0, 0.0); }
  let theta = F.eye.w * f32(dv);
  var d = ray_dir(vec2f(base) + 0.5 * f32(dv) + F.misc.zw);
  let w = water_hit(F.eye.xyz, d, theta, NEWTON);
  if (w.t < FAR) { return water_geo(d, w.t, w.kind, theta); }
  *fallback = true;
  d = ray_dir(pix(first));
  var t = abs(fw);
  let kind = select(1u, 2u, fw < 0.0);
  if (kind == 1u && !REF) { t = lake_refine(F.eye.xyz, d, t, theta, NEWTON); }
  return water_geo(d, t, kind, theta);
}

fn far_forest(r: vec3f) -> vec3f {
  let alb = vec3f(0.03, 0.05, 0.024);
  return alb * (F.sun_col.rgb * 0.3 * max(F.sun.y, 0.0) + sky_amb(0.2) * 0.6);
}

// ---- 4. reflect_pass ---------------------------------------------------------------------------------------

@compute @workgroup_size(8, 8)
fn reflect_pass(@builtin(global_invocation_id) gid: vec3u) {
  let dv = F.dims.z;
  var px: vec2u;
  if (!tile_px(gid, dv, &px)) { return; }
  C.limit = MAX_STEPS;
  var ok = false;
  var fb = false;
  let g = block_geo(px, dv, &ok, &fb);
  if (!ok) {
    if (dv > 1u) { textureStore(rt_w, px, vec4f(0.0)); }   // at full resolution nobody reads it
    return;
  }
  let ws0 = C.wsteps;
  let theta = F.eye.w * f32(dv);
  let p = F.eye.xyz + g.d * g.t;
  var r = reflect(g.d, g.n);
  r.y = max(r.y, 0.01);
  r = normalize(r);
  // The cone is the pixel's: roughness blurs a reflection, it doesn't make far objects hittable
  // from metres away (a roughness-widened tolerance fattened every reflected tree).
  let cone = Cone(g.t * theta, theta);
  let len = F.misc.y;
  let ro = p + vec3f(0.0, 0.02, 0.0);
  var h = Hit(INF, K_SKY, 0u);
  if (PROFILE != 2u) { h = trace_world(ro, r, 0.0, len, cone); }
  var col: vec3f;
  var kind = h.kind;
  if (h.kind != K_SKY) {
    if (PROFILE == 1u) { col = vec3f(0.05); } else { col = fog(shade_cheap(h, ro, r, cone), h.t, r); }
  } else {
    let pe = ro + r * len;
    if (len < FAR - 1.0 && length(pe.xz) < 138.0 && pe.y < terrain_h(pe.xz) + 13.0) {
      col = fog(far_forest(r), len, r);
      kind = K_FAR;
    } else {
      col = sky(r, false, cone.spread);
    }
  }
  if (VIEW == 1u) { col = heat(f32(C.steps + C.tsteps) / 64.0); }
  textureStore(rt_w, px, vec4f(col, g.t));
  if (STATS) {
    atomicAdd(&stats[S_REFL_RAYS], 1u);
    atomicAdd(&stats[S_REFL_STEPS], C.steps);
    atomicAdd(&stats[S_REFL_CAPS], C.caps);
    atomicAdd(&stats[S_REFL_TSTEPS], C.tsteps);
    atomicAdd(&stats[S_REFL_TCAPS], C.tcaps);
    atomicAdd(&stats[S_REFL_CELLS], C.cells);
    atomicAdd(&stats[S_REFL_SKY + kind], 1u);
    atomicAdd(&stats[S_REFL_WSTEPS], ws0);
    if (fb) { atomicAdd(&stats[S_REFL_FALLBACK], 1u); }
  }
}

// ---- 5. refract_pass ---------------------------------------------------------------------------------------

@compute @workgroup_size(8, 8)
fn refract_pass(@builtin(global_invocation_id) gid: vec3u) {
  let dv = F.dims.w;
  var px: vec2u;
  if (!tile_px(gid, dv, &px)) { return; }
  C.limit = MAX_STEPS;
  var ok = false;
  var fb = false;
  let g = block_geo(px, dv, &ok, &fb);
  if (!ok) {
    if (dv > 1u) { textureStore(qt_w, px, vec4f(0.0)); }
    return;
  }
  let ws0 = C.wsteps;
  let theta = F.eye.w * f32(dv);
  let p = F.eye.xyz + g.d * g.t;
  var rd = refract(g.d, g.n, ETA);
  if (dot(rd, rd) < 0.5) { rd = vec3f(g.d.x, -abs(g.d.y), g.d.z); }
  let cone = Cone(g.t * theta, theta);
  let h = trace_under(p, rd, REFR_LEN, cone);
  var col: vec3f;
  if (h.kind != K_SKY) {
    let pb = p + rd * h.t;
    col = through_water(shade_under(h, pb, fp_at(cone, h.t), p.y), h.t);
  } else {
    col = through_water(vec3f(0.0), REFR_LEN);
  }
  if (VIEW == 2u) { col = heat(f32(C.steps + C.tsteps) / 32.0); }
  textureStore(qt_w, px, vec4f(col, g.t));
  if (STATS) {
    atomicAdd(&stats[S_REFR_RAYS], 1u);
    atomicAdd(&stats[S_REFR_STEPS], C.steps);
    atomicAdd(&stats[S_REFR_CAPS], C.caps);
    atomicAdd(&stats[S_REFR_TSTEPS], C.tsteps);
    atomicAdd(&stats[S_REFR_TCAPS], C.tcaps);
    atomicAdd(&stats[S_REFR_WSTEPS], ws0);
    if (h.kind != K_SKY) { atomicAdd(&stats[S_REFR_HIT], 1u); }
  }
}

// ---- 6. composite ------------------------------------------------------------------------------------------

/** Reflection or refraction at a full-resolution pixel: exact at full resolution; otherwise
 *  bilinear over the valid (water) texels around it, falling back to the nearest valid one. */
fn fetch(tex: texture_2d<f32>, g: vec2u, dv: u32) -> vec4f {
  if (dv == 1u) { return vec4f(textureLoad(tex, g, 0).rgb, 1.0); }
  let lw = vec2i((F.dims.xy + vec2u(dv - 1u)) / dv);
  let lp = (vec2f(g) + 0.5) / f32(dv) - 0.5;
  let b = vec2i(floor(lp));
  let f = lp - floor(lp);
  var acc = vec3f(0.0);
  var wsum = 0.0;
  for (var k = 0; k < 4; k++) {
    let o = vec2i(k & 1, k >> 1);
    let q = clamp(b + o, vec2i(0), lw - 1);
    let s = textureLoad(tex, q, 0);
    let w = select(1.0 - f.x, f.x, o.x == 1) * select(1.0 - f.y, f.y, o.y == 1) * select(0.0, 1.0, s.a > 0.0);
    acc += s.rgb * w;
    wsum += w;
  }
  if (wsum > 1e-4) { return vec4f(acc / wsum, 1.0); }
  var best = 1e9;
  var c = vec4f(0.0);
  for (var j = -1; j <= 2; j++) {
    for (var i = -1; i <= 2; i++) {
      let q = clamp(b + vec2i(i, j), vec2i(0), lw - 1);
      let s = textureLoad(tex, q, 0);
      let dd = dot(vec2f(q) - lp, vec2f(q) - lp);
      if (s.a > 0.0 && dd < best) { best = dd; c = vec4f(s.rgb, 1.0); }
    }
  }
  return c;
}

/** The sun's specular from the water, through an anisotropic Beckmann lobe whose slope variance
 *  includes what filtering removed (Toksvig-style), so the glint widens where waves were faded
 *  out instead of aliasing. Returns BRDF × cos, to multiply by the sun's irradiance colour. */
fn glint(n: vec3f, v: vec3f, vpar: f32, vperp: f32, vd: vec2f) -> f32 {
  let l = F.sun.xyz;
  if (dot(n, l) <= 0.0) { return 0.0; }
  let h = normalize(l + v);
  let ndh = max(dot(n, h), 1e-3);
  let tv0 = vec3f(vd.x, 0.0, vd.y);
  let tv = normalize(tv0 - n * dot(n, tv0));
  let bv = cross(n, tv);
  let sx = dot(h, tv) / ndh;
  let sy = dot(h, bv) / ndh;
  let sun2 = 0.25 * F.sun.w * F.sun.w;
  let vx = BASE_VAR + vpar + sun2;
  let vy = BASE_VAR + vperp + sun2;
  let dd = exp(-0.5 * (sx * sx / vx + sy * sy / vy)) / (6.2831853 * sqrt(vx * vy) * ndh * ndh * ndh * ndh);
  let fr = 0.02 + 0.98 * pow(1.0 - clamp(dot(h, v), 0.0, 1.0), 5.0);
  return dd * fr / (4.0 * max(dot(n, v), 0.05));
}

@compute @workgroup_size(8, 8)
fn composite(@builtin(global_invocation_id) gid: vec3u) {
  var px: vec2u;
  if (!tile_px(gid, 1u, &px)) { return; }
  if (textureLoad(wm_r, px, 0).x == 0.0) { return; }
  C.limit = MAX_STEPS;
  let g = read_geo(px);
  let d = g.d;
  let n = g.n;
  let t = g.t;
  let p = F.eye.xyz + d * t;
  let vd = view_xz(d);
  let v = -d;
  let fr = 0.02 + 0.98 * pow(1.0 - clamp(dot(v, n), 0.0, 1.0), 5.0);
  // Fallbacks (no valid texel nearby) are rare, so they're computed only when needed.
  let rf = fetch(rt_r, px, F.dims.z);
  var refl = rf.rgb;
  if (rf.a == 0.0) { let rdir = reflect(d, n); refl = sky(vec3f(rdir.x, max(rdir.y, 0.01), rdir.z), false, F.eye.w); }
  let qf = fetch(qt_r, px, F.dims.w);
  var refr = qf.rgb;
  if (qf.a == 0.0) { refr = through_water(vec3f(0.0), REFR_LEN); }
  var col = refl * fr + refr * (1.0 - fr);
  let fp = surface_fp(p, d, t, F.eye.w, g.kind);
  let fa = foam(p, g.kind, max(fp.x, fp.y));
  let gl = glint(n, v, g.vpar, g.vperp, vd);
  var vis = 1.0;
  if (gl * lum(F.sun_col.rgb) * F.sun_col.w > 0.01 || fa > 0.02) {
    vis = sun_vis(p, vec3f(0.0, 1.0, 0.0), F.hor_sun.w, 0.02);
    if (STATS) { atomicAdd(&stats[S_GLINT_RAYS], 1u); atomicAdd(&stats[S_GLINT_STEPS], C.shsteps); }
  }
  col += F.sun_col.rgb * gl * vis * (1.0 - fa);
  if (fa > 0.0) {
    let fl = vec3f(0.62, 0.64, 0.62) * (F.sun_col.rgb * max(dot(n, F.sun.xyz), 0.15) * (0.35 + 0.65 * vis) + sky_amb(n.y));
    col = mix(col, fl, fa * fa * (3.0 - 2.0 * fa));
  }
  col = fog(col, t, d);
  var outc = tonemap(col);
  if (VIEW == 1u) { outc = refl; }
  if (VIEW == 2u) { outc = refr; }
  textureStore(out_w, px, vec4f(outc, 1.0));
}
