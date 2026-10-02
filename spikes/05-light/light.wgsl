// Every lighting pass, primary visibility, load-time cooking and probe placement. Appended to
// common.wgsl + scenes.wgsl; ref.wgsl follows. Bindings in group 1 are numbered uniquely across the
// module, and each pass's explicit layout (main.js) lists only the ones it uses.

override FIELD: u32 = 0u;       // what sky cones and probe rays trace: 0 analytic, 1 cooked clipmap, 2 hybrid
override SUN_FIELD: u32 = 0u;   // what sun cones trace (same encoding)
override VIS: u32 = 2u;         // probe gather visibility: 0 none (trilinear only), 2 DDGI (backface + Chebyshev)
override SKY_SRC: u32 = 0u;     // composite's sky light: 0 per-pixel cones, 1 probes, 2 probes × cone AO
override SKY_AO: bool = false;  // sky pass writes short-range cone visibility (AO) instead of sky light
override INLINE_GATHER: bool = false;   // composite gathers the probes itself (no gather pass, no bounce/skyp targets)

const IRR_N: u32 = 6u;          // octahedral irradiance tile interior (8×8 with border)
const DEP_N: u32 = 14u;         // octahedral distance-moment tile interior (16×16 with border)
const TPR: u32 = 128u;          // tiles per atlas row
const MAX_R: u32 = 64u;         // ray-buffer stride per probe

@group(1) @binding(0) var g_t_w: texture_storage_2d<r32float, write>;
@group(1) @binding(1) var g_n_w: texture_storage_2d<rgba16float, write>;
@group(1) @binding(2) var g_a_w: texture_storage_2d<rgba8unorm, write>;
@group(1) @binding(3) var g_t: texture_2d<f32>;
@group(1) @binding(4) var g_n: texture_2d<f32>;
@group(1) @binding(5) var g_a: texture_2d<f32>;
@group(1) @binding(6) var sun_w: texture_storage_2d<r32float, write>;
@group(1) @binding(7) var sun_t: texture_2d<f32>;
@group(1) @binding(8) var sky_w: texture_storage_2d<rgba16float, write>;
@group(1) @binding(9) var sky_t: texture_2d<f32>;
@group(1) @binding(10) var hist_prev: texture_2d<f32>;
@group(1) @binding(11) var hd_prev: texture_2d<f32>;
@group(1) @binding(12) var hist_w: texture_storage_2d<rgba16float, write>;
@group(1) @binding(13) var hd_w: texture_storage_2d<r32float, write>;
@group(1) @binding(14) var hist_t: texture_2d<f32>;
@group(1) @binding(15) var hd_t: texture_2d<f32>;
@group(1) @binding(16) var<storage, read> probes: array<vec4f>;
@group(1) @binding(17) var<storage, read_write> probes_rw: array<vec4f>;
@group(1) @binding(18) var<storage, read_write> rays_w: array<vec4f>;
@group(1) @binding(19) var<storage, read> rays: array<vec4f>;
@group(1) @binding(20) var irr_s_t: texture_2d<f32>;
@group(1) @binding(21) var irr_b_t: texture_2d<f32>;
@group(1) @binding(22) var dep_t: texture_2d<f32>;
@group(1) @binding(23) var irr_s_w: texture_storage_2d<rgba16float, write>;
@group(1) @binding(24) var irr_b_w: texture_storage_2d<rgba16float, write>;
@group(1) @binding(25) var dep_w: texture_storage_2d<rgba16float, write>;
// 26: lin, 27: vol_t (scenes.wgsl)
@group(1) @binding(28) var vol_w: texture_storage_3d<VOLFMT, write>;
@group(1) @binding(29) var bounce_w: texture_storage_2d<rgba16float, write>;
@group(1) @binding(30) var skyp_w: texture_storage_2d<rgba16float, write>;
@group(1) @binding(31) var bounce_t: texture_2d<f32>;
@group(1) @binding(32) var skyp_t: texture_2d<f32>;
@group(1) @binding(33) var out_w: texture_storage_2d<rgba16float, write>;

// ---- shared helpers ----------------------------------------------------------------------------------

fn px_dir(q: vec2u) -> vec3f { return ray_dir(vec2f(q) + 0.5, f32(F.dims.x), f32(F.dims.y)); }

/** Normal-direction offset for rays leaving a primary hit: a few millimetres plus two pixel footprints. */
fn surf_bias(t: f32) -> f32 { return 0.003 + 2.0 * F.eye.w * t; }

/** Fog (aerial perspective): deterministic, identical in the real-time path and the reference. */
fn fog_apply(l: vec3f, t: f32, d: vec3f) -> vec3f {
  if (F.fog.w <= 0.0) { return l; }
  let tr = exp(-F.fog.w * t);
  let mu = max(dot(d, F.sun.xyz), 0.0);
  let ins = F.fog.rgb + F.sun_e.rgb * (0.03 * pow(mu, 10.0));
  return l * tr + ins * (1.0 - tr);
}

/** Final colour of a surface pixel from its three irradiance terms. The view mode isolates one. */
fn shade_px(a: vec3f, t: f32, d: vec3f, e_sun: vec3f, e_sky: vec3f, e_b: vec3f) -> vec3f {
  var e = e_sun + e_sky + e_b;
  if (F.flags.y == 1u) { e = e_sun; }
  if (F.flags.y == 2u) { e = e_sky; }
  if (F.flags.y == 3u) { e = e_b; }
  var l = a / PI * e;
  if (F.flags.y == 0u) { l = fog_apply(l, t, d); }
  return tonemap(l * F.sun_e.w);
}

fn background(d: vec3f) -> vec3f {
  var l = sky_radiance(d);
  let cs = inverseSqrt(1.0 + F.sun.w * F.sun.w);
  if (dot(d, F.sun.xyz) > cs) { l += F.sun_e.rgb / (PI * F.sun.w * F.sun.w); }
  // Haze toward the horizon, so the sky meets distant fogged terrain without a seam.
  l = fog_apply(l, world_r() * (1.0 - smoothstep(0.0, 0.25, d.y)), d);
  return tonemap(l * F.sun_e.w);
}

// ---- probes -----------------------------------------------------------------------------------------------

fn probe_grid_pos(i: u32) -> vec3f {
  let nx = F.pg_n.x;
  let ny = F.pg_n.y;
  return F.pg_o.xyz + vec3f(f32(i % nx), f32((i / nx) % ny), f32(i / (nx * ny))) * F.pg_s.xyz;
}

fn probe_index(c: vec3u) -> u32 { return (c.z * F.pg_n.y + c.y) * F.pg_n.x + c.x; }

fn tile_origin(i: u32, size: u32) -> vec2u { return vec2u(i % TPR, i / TPR) * size; }

fn atlas_uv(i: u32, dir: vec3f, n: u32, size: vec2f) -> vec2f {
  let e = oct_encode(dir) * 0.5 + 0.5;
  return (vec2f(tile_origin(i, n + 2u)) + 1.0 + e * f32(n)) / size;
}

struct PG { sky: vec3f, bounce: vec3f }

/** Irradiance (÷π) at p with normal n from the 8 surrounding probes, DDGI-style: trilinear weights,
 *  a smooth backface term and a Chebyshev visibility test on the probes' distance moments. */
fn probe_gather(p: vec3f, n: vec3f, v: vec3f, want_bounce: bool) -> PG {
  let sp = F.pg_s.xyz;
  let mins = min(sp.x, min(sp.y, sp.z));
  let bp = p + (n * F.pa.x + v * F.pa.y) * mins;
  let gc = (bp - F.pg_o.xyz) / sp;
  let hi = max(vec3f(F.pg_n.xyz) - 2.0, vec3f(0.0));
  let base = clamp(floor(gc), vec3f(0.0), hi);
  let a = clamp(gc - base, vec3f(0.0), vec3f(1.0));
  let isz = vec2f(textureDimensions(irr_s_t));
  let dsz = vec2f(textureDimensions(dep_t));
  var ss = vec3f(0.0);
  var sb = vec3f(0.0);
  var ws = 0.0;
  for (var k = 0u; k < 8u; k++) {
    let off = vec3u(k & 1u, (k >> 1u) & 1u, k >> 2u);
    let c = vec3u(base) + off;
    let pi = probe_index(c);
    let st = probes[pi];
    if (st.w < 0.5) { continue; }
    let tri = mix(1.0 - a, a, vec3f(off));
    var w = tri.x * tri.y * tri.z;
    if (VIS != 0u) {
      let pp = F.pg_o.xyz + vec3f(c) * sp + st.xyz;
      let tp = pp - bp;
      let dist = length(tp);
      let dir = tp / max(dist, 1e-4);
      let bf = (dot(dir, n) + 1.0) * 0.5;
      var wv = bf * bf + 0.2;
      let m = textureSampleLevel(dep_t, lin, atlas_uv(pi, -dir, DEP_N, dsz), 0.0).rg;
      let dn = dist / F.pg_s.w;
      if (dn > m.x) {
        let va = abs(m.y - m.x * m.x);
        let dd = dn - m.x;
        var ch = va / (va + dd * dd);
        ch = ch * ch * ch;
        wv *= max(ch, 0.05);
      }
      wv = max(wv, 1e-6);
      if (wv < 0.2) { wv *= wv * wv / 0.04; }
      w *= wv;
    }
    let uv = atlas_uv(pi, n, IRR_N, isz);
    ss += w * textureSampleLevel(irr_s_t, lin, uv, 0.0).rgb;
    if (want_bounce) { sb += w * textureSampleLevel(irr_b_t, lin, uv, 0.0).rgb; }
    ws += w;
  }
  if (ws < 1e-6) { return PG(vec3f(0.0), vec3f(0.0)); }
  return PG(ss / ws, sb / ws);
}

// ---- primary visibility (not lighting) -----------------------------------------------------------------

@compute @workgroup_size(8, 8)
fn primary(@builtin(global_invocation_id) gid0: vec3u) {
  let gid = gid0.xy + vec2u(F.rf.w & 0xffffu, F.rf.w >> 16u);
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let px = vec2i(gid.xy);
  let d = px_dir(gid.xy);
  let h = march_primary(F.eye.xyz, d, F.eye.w);
  stat(S_PRIM_PX, 1u);
  stat(S_PRIM_STEPS, h.steps);
  stat(S_PRIM_CAPS, select(0u, 1u, h.status == 2u));
  if (h.status == 0u) {
    textureStore(g_t_w, px, vec4f(-1.0));
    textureStore(g_n_w, px, vec4f(0.0));
    textureStore(g_a_w, px, vec4f(0.0));
    return;
  }
  stat(S_PRIM_HITS, 1u);
  let p = F.eye.xyz + d * h.t;
  let n = surface_normal(p, max(1e-3, 0.5 * F.eye.w * h.t));
  let id = surface_id(p);
  let a = albedo(p, n, id);
  textureStore(g_t_w, px, vec4f(h.t));
  textureStore(g_n_w, px, vec4f(n, id));
  textureStore(g_a_w, px, vec4f(a, 1.0));
}

// ---- sun: one distance-field cone toward the sun disc per pixel -------------------------------------------

@compute @workgroup_size(8, 8)
fn sun(@builtin(global_invocation_id) gid0: vec3u) {
  let gid = gid0.xy + vec2u(0u, F.rf.w >> 16u);
  let ss = F.rs.x;
  if (gid.x * ss >= F.dims.x || gid.y * ss >= F.dims.y) { return; }
  let q = gid.xy * ss;
  let t = textureLoad(g_t, vec2i(q), 0).r;
  var vis = 0.0;
  if (t > 0.0) {
    let n = textureLoad(g_n, vec2i(q), 0).xyz;
    if (dot(n, F.sun.xyz) > 0.0) {
      let p = F.eye.xyz + px_dir(q) * t;
      let o = p + n * surf_bias(t);
      let c = cone_trace(o, F.sun.xyz, F.sun.w, 0.01, F.lp.y, SUN_STEPS, SUN_FIELD, true, F.pa.z, !SUN_TMAP);
      vis = c.vis;
      if (SUN_TMAP) { vis = min(vis, terrain_sun(o)); }
      stat(S_SUN_RAYS, 1u);
      stat(S_SUN_STEPS, c.steps);
      stat(S_SUN_CAPS, select(0u, 1u, c.cap));
    }
  }
  textureStore(sun_w, vec2i(gid.xy), vec4f(vis));
}

// ---- sky: K distance-field cones per pixel, rotated per pixel per frame ---------------------------------

/** Width (sine of the half-angle) of each of K cones that tile the hemisphere: 2π/K steradians each. */
fn sky_cone_w(k: u32) -> f32 {
  if (k <= 1u) { return 0.866; }
  let c = 1.0 - 1.0 / f32(k);
  return sqrt(1.0 - c * c);
}

@compute @workgroup_size(8, 8)
fn sky(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= F.dims.z || gid.y >= F.dims.w) { return; }
  let scale = F.dims.x / F.dims.z;
  let q = gid.xy * scale;
  let t = textureLoad(g_t, vec2i(q), 0).r;
  if (t <= 0.0) { textureStore(sky_w, vec2i(gid.xy), vec4f(0.0)); return; }
  let n = textureLoad(g_n, vec2i(q), 0).xyz;
  let p = F.eye.xyz + px_dir(q) * t;
  let o = p + n * surf_bias(t);
  let kn = F.fr.w;
  let w = sky_cone_w(kn);
  let rnd = ign(vec2f(gid.xy), F.fr.x);
  let b = basis(n);
  // A purely cooked cone starts a voxel and a half out, past the surface's own trilinear blur.
  let t0 = select(0.05, 1.5 * F.vol_o.w, FIELD == F_COOKED);
  var e = vec3f(0.0);
  var steps = 0u;
  var caps = 0u;
  for (var k = 0u; k < kn; k++) {
    var axis = n;
    var sin_e = 1.0;
    if (kn > 1u) {
      let u1 = (f32(k) + 0.5) / f32(kn);
      let r = sqrt(u1);
      sin_e = sqrt(1.0 - u1);
      let ph = 2.0 * PI * (f32(k) * 0.618034 + rnd);
      axis = b * vec3f(r * cos(ph), r * sin(ph), sin_e);
    }
    let c = cone_trace(o, axis, w, t0, F.lp.x, SKY_STEPS, FIELD, false, F.pa.w, true);
    // Divide out what the tangent plane alone would occlude, so open flat ground reads as open.
    let rp = clamp(sin_e / w, 0.0, 1.0);
    let vp = rp * rp * (3.0 - 2.0 * rp);
    let v = min(c.vis / max(vp, 0.05), 1.0);
    if (SKY_AO) { e += vec3f(v); } else { e += sky_radiance(axis) * v; }
    steps += c.steps;
    caps += select(0u, 1u, c.cap);
  }
  stat(S_SKY_CONES, kn);
  stat(S_SKY_STEPS, steps);
  stat(S_SKY_CAPS, caps);
  if (SKY_AO) { e /= f32(kn); } else { e *= PI / f32(kn); }
  textureStore(sky_w, vec2i(gid.xy), vec4f(e, 1.0));
}

// ---- temporal accumulation of sky light (reprojected, with depth-based disocclusion) -----------------------

@compute @workgroup_size(8, 8)
fn temporal(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= F.dims.z || gid.y >= F.dims.w) { return; }
  let s = vec2i(gid.xy);
  let scale = F.dims.x / F.dims.z;
  let q = gid.xy * scale;
  let t = textureLoad(g_t, vec2i(q), 0).r;
  let cur = textureLoad(sky_t, s, 0);
  if (t <= 0.0) {
    textureStore(hist_w, s, vec4f(0.0));
    textureStore(hd_w, s, vec4f(-1.0));
    return;
  }
  let pw = F.eye.xyz + px_dir(q) * t;
  var hist = vec4f(0.0);
  var ok = false;
  if (F.flags.z == 0u) {
    let rel = pw - F.pe.xyz;
    let z = dot(rel, F.pcf.xyz);
    if (z > 0.0) {
      let u = dot(rel, F.pcr.xyz) / (z * dot(F.pcr.xyz, F.pcr.xyz));
      let v = dot(rel, F.pcu.xyz) / (z * dot(F.pcu.xyz, F.pcu.xyz));
      let fx = ((u * 0.5 + 0.5) * f32(F.dims.x) - 0.5) / f32(scale);
      let fy = ((0.5 - v * 0.5) * f32(F.dims.y) - 0.5) / f32(scale);
      let f0 = floor(vec2f(fx, fy));
      let fr = vec2f(fx, fy) - f0;
      let dist = length(rel);
      var acc = vec4f(0.0);
      var ws = 0.0;
      for (var k = 0; k < 4; k++) {
        let c = vec2i(f0) + vec2i(k & 1, k >> 1);
        if (c.x < 0 || c.y < 0 || c.x >= i32(F.dims.z) || c.y >= i32(F.dims.w)) { continue; }
        let hd = textureLoad(hd_prev, c, 0).r;
        if (hd <= 0.0 || abs(hd - dist) > 0.03 * dist + 0.05) { continue; }
        let bw = select(1.0 - fr.x, fr.x, (k & 1) == 1) * select(1.0 - fr.y, fr.y, k >= 2);
        acc += bw * textureLoad(hist_prev, c, 0);
        ws += bw;
      }
      if (ws > 0.01) { hist = acc / ws; ok = true; }
    }
  }
  let age = min(select(0.0, hist.a, ok) + 1.0, f32(F.fr.y));
  textureStore(hist_w, s, vec4f(mix(hist.rgb, cur.rgb, 1.0 / age), age));
  textureStore(hd_w, s, vec4f(t));
}

// ---- probe rays: one bounce, sky light at the hit from the sky-only probe set -------------------------------

@compute @workgroup_size(64)
fn probe_trace(@builtin(global_invocation_id) gid: vec3u) {
  let rn = F.fr.z;
  let probe = gid.x / rn;
  let ray = gid.x % rn;
  if (probe >= F.pg_n.w) { return; }
  let st = probes[probe];
  if (st.w < 0.5) { return; }
  let slot = (probe * MAX_R + ray) * 2u;
  let o = probe_grid_pos(probe) + st.xyz;
  let d = probe_ray_dir(ray, rn);
  let h = march_hit(o, d, F.lp.z, PROBE_STEPS, FIELD, 0.001, 0.01, 0.0);
  var bounce = vec3f(0.0);
  var skyr = vec3f(0.0);
  var dist = h.t;
  var shs = 0u;
  var shc = 0u;
  var shr = 0u;
  var back = 0u;
  if (h.status == 0u) {
    skyr = sky_radiance(d);
    dist = F.lp.z;
  } else if (h.status == 1u) {
    let y = o + d * h.t;
    let n = surface_normal(y, 0.01);
    if (dot(n, d) > 0.0) {
      back = 1u;
    } else {
      var e = vec3f(0.0);
      let ndl = dot(n, F.sun.xyz);
      if (ndl > 0.0) {
        let c = cone_trace(y + n * 0.02, F.sun.xyz, F.sun.w, 0.02, F.lp.y, PSHADOW_STEPS, FIELD, true, F.pa.z, !SUN_TMAP);
        var sv = c.vis;
        if (SUN_TMAP) { sv = min(sv, terrain_sun(y + n * 0.02)); }
        e += F.sun_e.rgb * ndl * sv;
        shr = 1u;
        shs = c.steps;
        shc = select(0u, 1u, c.cap);
      }
      e += PI * probe_gather(y, n, -d, false).sky;
      bounce = albedo(y, n, surface_id(y)) / PI * e;
    }
  }
  rays_w[slot] = vec4f(bounce, dist);
  rays_w[slot + 1u] = vec4f(skyr, 0.0);
  stat(S_PR_RAYS, 1u);
  stat(S_PR_STEPS, h.steps);
  stat(S_PR_CAPS, select(0u, 1u, h.status == 2u));
  stat(S_PR_HITS, select(0u, 1u, h.status == 1u));
  stat(S_PR_BACK, back);
  stat(S_PS_RAYS, shr);
  stat(S_PS_STEPS, shs);
  stat(S_PS_CAPS, shc);
}

var<workgroup> w_r0: array<vec4f, MAX_R>;
var<workgroup> w_r1: array<vec4f, MAX_R>;
var<workgroup> w_d: array<vec4f, MAX_R>;

/** Probe irradiance update: one workgroup per probe, one invocation per texel of its 8×8 tiles. */
@compute @workgroup_size(8, 8)
fn probe_irr(@builtin(workgroup_id) wg: vec3u, @builtin(local_invocation_id) lid: vec3u,
             @builtin(local_invocation_index) li: u32) {
  let probe = wg.y * TPR + wg.x;
  let rn = F.fr.z;
  let valid = probe < F.pg_n.w;
  if (valid && li < rn) {
    let b = (probe * MAX_R + li) * 2u;
    w_r0[li] = rays[b];
    w_r1[li] = rays[b + 1u];
    w_d[li] = vec4f(probe_ray_dir(li, rn), 0.0);
  }
  workgroupBarrier();
  if (!valid) { return; }
  let texel = vec2i(tile_origin(probe, IRR_N + 2u) + lid.xy);
  var s = vec3f(0.0);
  var b = vec3f(0.0);
  if (probes[probe].w > 0.5) {
    let dir = oct_texel_dir(oct_border_src(lid.xy, IRR_N), IRR_N);
    var ws = 0.0;
    for (var r = 0u; r < rn; r++) {
      let w = max(dot(dir, w_d[r].xyz), 0.0);
      b += w * w_r0[r].rgb;
      s += w * w_r1[r].rgb;
      ws += w;
    }
    b /= max(ws, 1e-4);
    s /= max(ws, 1e-4);
    if (F.flags.z == 0u) {
      b = mix(textureLoad(irr_b_t, texel, 0).rgb, b, F.pg_o.w);
      s = mix(textureLoad(irr_s_t, texel, 0).rgb, s, F.pg_o.w);
    }
  }
  textureStore(irr_b_w, texel, vec4f(b, 1.0));
  textureStore(irr_s_w, texel, vec4f(s, 1.0));
}

/** Probe distance moments (mean, mean²) for the Chebyshev test: one workgroup per probe. */
@compute @workgroup_size(16, 16)
fn probe_depth(@builtin(workgroup_id) wg: vec3u, @builtin(local_invocation_id) lid: vec3u,
               @builtin(local_invocation_index) li: u32) {
  let probe = wg.y * TPR + wg.x;
  let rn = F.fr.z;
  let valid = probe < F.pg_n.w;
  if (valid && li < rn) {
    w_r0[li] = rays[(probe * MAX_R + li) * 2u];
    w_d[li] = vec4f(probe_ray_dir(li, rn), 0.0);
  }
  workgroupBarrier();
  if (!valid) { return; }
  let texel = vec2i(tile_origin(probe, DEP_N + 2u) + lid.xy);
  var m = vec2f(1.0, 1.0);
  if (probes[probe].w > 0.5) {
    let dir = oct_texel_dir(oct_border_src(lid.xy, DEP_N), DEP_N);
    var acc = vec2f(0.0);
    var ws = 0.0;
    for (var r = 0u; r < rn; r++) {
      let w = pow(max(dot(dir, w_d[r].xyz), 1e-6), 50.0);
      let dn = min(w_r0[r].w, F.pg_s.w) / F.pg_s.w;
      acc += w * vec2f(dn, dn * dn);
      ws += w;
    }
    if (ws > 1e-6) { m = acc / ws; }
    if (F.flags.z == 0u) { m = mix(textureLoad(dep_t, texel, 0).rg, m, F.pg_o.w); }
  }
  textureStore(dep_w, texel, vec4f(m, 0.0, 1.0));
}

// ---- gather: bounce (and probe sky) irradiance per pixel --------------------------------------------------

@compute @workgroup_size(8, 8)
fn gather(@builtin(global_invocation_id) gid: vec3u) {
  let gs = F.rs.y;
  if (gid.x * gs >= F.dims.x || gid.y * gs >= F.dims.y) { return; }
  let q = gid.xy * gs;
  let t = textureLoad(g_t, vec2i(q), 0).r;
  var g = PG(vec3f(0.0), vec3f(0.0));
  if (t > 0.0) {
    let d = px_dir(q);
    g = probe_gather(F.eye.xyz + d * t, textureLoad(g_n, vec2i(q), 0).xyz, -d, true);
  }
  textureStore(bounce_w, vec2i(gid.xy), vec4f(g.bounce * PI, 1.0));
  textureStore(skyp_w, vec2i(gid.xy), vec4f(g.sky * PI, 1.0));
}

/** Upsampling weights for one full-resolution pixel from a half-resolution grid (sample c taken at
 *  full-resolution pixel 2·c): bilinear × depth similarity × normal similarity, computed once and
 *  applied to every half-resolution input (sky history, bounce, probe sky, sun). */
struct UpW { c: array<vec2i, 4>, w: vec4f, near: u32 }

fn up_weights(px: vec2i, t: f32, n: vec3f) -> UpW {
  var u: UpW;
  let s = vec2f(px) * 0.5;
  let s0 = vec2i(floor(s));
  let f = s - vec2f(s0);
  let hi = vec2i((vec2u(F.dims.xy) + 1u) / 2u) - 1;
  var nd = INF;
  var ws = 0.0;
  for (var k = 0; k < 4; k++) {
    let c = min(s0 + vec2i(k & 1, k >> 1), hi);
    u.c[k] = c;
    let q = c * 2;
    let tq = textureLoad(g_t, q, 0).r;
    var w = 0.0;
    if (tq > 0.0) {
      let dz = abs(tq - t);
      if (dz < nd) { nd = dz; u.near = u32(k); }
      let bw = select(1.0 - f.x, f.x, (k & 1) == 1) * select(1.0 - f.y, f.y, k >= 2);
      let nw = pow(max(dot(n, textureLoad(g_n, q, 0).xyz), 0.0), 8.0);
      w = bw * nw * exp(-dz / (0.02 * t + 0.02)) + 1e-5 * bw;
    }
    u.w[k] = w;
    ws += w;
  }
  if (ws < 1e-4) { u.w = vec4f(0.0); u.w[u.near] = 1.0; } else { u.w /= ws; }
  return u;
}

fn up_apply(tex: texture_2d<f32>, u: UpW) -> vec4f {
  return u.w.x * textureLoad(tex, u.c[0], 0) + u.w.y * textureLoad(tex, u.c[1], 0)
       + u.w.z * textureLoad(tex, u.c[2], 0) + u.w.w * textureLoad(tex, u.c[3], 0);
}

/** A low-resolution term at a full-resolution pixel: bilinear over the four nearest samples (each
 *  taken at full-resolution pixel 2·c), weighted by depth and normal similarity. */
fn upsample(tex: texture_2d<f32>, px: vec2i, t: f32, n: vec3f, scale: u32) -> vec4f {
  if (scale == 1u) { return textureLoad(tex, px, 0); }
  let s = vec2f(px) / f32(scale);
  let s0 = vec2i(floor(s));
  let f = s - vec2f(s0);
  let hi = vec2i((vec2u(F.dims.xy) + scale - 1u) / scale) - 1;
  var acc = vec4f(0.0);
  var ws = 0.0;
  var near = vec4f(0.0);
  var nd = INF;
  for (var k = 0; k < 4; k++) {
    let c = min(s0 + vec2i(k & 1, k >> 1), hi);
    let q = c * i32(scale);
    let tq = textureLoad(g_t, q, 0).r;
    if (tq <= 0.0) { continue; }
    let v = textureLoad(tex, c, 0);
    let dz = abs(tq - t);
    if (dz < nd) { nd = dz; near = v; }
    let nq = textureLoad(g_n, q, 0).xyz;
    let bw = select(1.0 - f.x, f.x, (k & 1) == 1) * select(1.0 - f.y, f.y, k >= 2);
    let nw = pow(max(dot(n, nq), 0.0), 8.0);
    let w = bw * nw * exp(-dz / (0.02 * t + 0.02)) + 1e-5 * bw;
    acc += w * v;
    ws += w;
  }
  if (ws < 1e-4) { return near; }
  return acc / ws;
}

// ---- composite ------------------------------------------------------------------------------------------------

/** Sky light at a full-resolution pixel from the (possibly half-resolution) accumulated sky, with a
 *  depth-aware bilinear upsample. */
fn upsample_sky(px: vec2i, t: f32) -> vec3f {
  let scale = F.dims.x / F.dims.z;
  if (scale == 1u) { return textureLoad(hist_t, px, 0).rgb; }
  let s = vec2f(px) / f32(scale);
  let s0 = vec2i(floor(s));
  let f = s - vec2f(s0);
  var acc = vec3f(0.0);
  var ws = 0.0;
  var near = vec3f(0.0);
  var nd = INF;
  for (var k = 0; k < 4; k++) {
    let c = min(s0 + vec2i(k & 1, k >> 1), vec2i(i32(F.dims.z) - 1, i32(F.dims.w) - 1));
    let hd = textureLoad(hd_t, c, 0).r;
    if (hd <= 0.0) { continue; }
    let h = textureLoad(hist_t, c, 0).rgb;
    let dz = abs(hd - t);
    if (dz < nd) { nd = dz; near = h; }
    let bw = select(1.0 - f.x, f.x, (k & 1) == 1) * select(1.0 - f.y, f.y, k >= 2);
    let w = bw * exp(-dz / (0.02 * t + 0.02)) + 1e-5 * bw;
    acc += w * h;
    ws += w;
  }
  if (ws < 1e-4) { return near; }
  return acc / ws;
}

@compute @workgroup_size(8, 8)
fn composite(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let px = vec2i(gid.xy);
  let t = textureLoad(g_t, px, 0).r;
  let d = px_dir(gid.xy);
  if (t <= 0.0) { textureStore(out_w, px, vec4f(select(background(d), vec3f(0.0, 0.0, 1.0), F.flags.y == 6u), 1.0)); return; }
  let n = textureLoad(g_n, px, 0).xyz;
  let a = textureLoad(g_a, px, 0).rgb;
  // One set of half-resolution weights serves every half-resolution input.
  let skyHalf = F.dims.z < F.dims.x;
  var uw: UpW;
  if (skyHalf || F.rs.x == 2u || F.rs.y == 2u) { uw = up_weights(px, t, n); }
  var vis: f32;
  if (F.rs.x == 2u) { vis = up_apply(sun_t, uw).r; } else { vis = textureLoad(sun_t, px, 0).r; }
  let e_sun = F.sun_e.rgb * max(dot(n, F.sun.xyz), 0.0) * vis;
  var e_sky: vec3f;
  var e_b: vec3f;
  var sky_c = vec3f(0.0);
  if (SKY_SRC != 1u) { if (skyHalf) { sky_c = up_apply(hist_t, uw).rgb; } else { sky_c = textureLoad(hist_t, px, 0).rgb; } }
  if (INLINE_GATHER) {
    let g = probe_gather(F.eye.xyz + d * t, n, -d, true);
    e_b = g.bounce * PI;
    if (SKY_SRC == 0u) { e_sky = sky_c; } else { e_sky = g.sky * PI; }
  } else if (F.rs.y == 2u) {
    e_b = up_apply(bounce_t, uw).rgb;
    if (SKY_SRC == 0u) { e_sky = sky_c; } else { e_sky = up_apply(skyp_t, uw).rgb; }
  } else {
    e_b = textureLoad(bounce_t, px, 0).rgb;
    if (SKY_SRC == 0u) { e_sky = sky_c; } else { e_sky = textureLoad(skyp_t, px, 0).rgb; }
  }
  // Probes carry the sky and bounce light; short cones add contact occlusion on top (sky_c holds
  // their visibility in this mode). Like production DDGI + AO, this double-counts some occlusion.
  if (SKY_SRC == 2u) { e_sky *= sky_c.r; e_b *= sky_c.r; }
  var c = shade_px(a, t, d, e_sun, e_sky, e_b);
  // Debug views (development only): 4 albedo, 5 normal, 6 depth, 7 sun visibility, 8 sky, 9 bounce.
  switch F.flags.y {
    case 4u: { c = a; }
    case 5u: { c = n * 0.5 + 0.5; }
    case 6u: { c = vec3f(fract(log2(t)), fract(log2(t) * 0.25), 0.5); }
    case 7u: { c = vec3f(upsample(sun_t, px, t, n, F.rs.x).r); }
    case 8u: { c = e_sky * 0.5; }
    case 9u: { c = e_b * 0.5; }
    case 11u: {
      let p = F.eye.xyz + d * t;
      c = vec3f(clamp(p.y * 4.0, 0.0, 1.0), clamp(-p.y * 40.0, 0.0, 1.0), clamp(p.y * 40.0, 0.0, 1.0));
    }
    case 10u: {
      let id = textureLoad(g_n, px, 0).w;
      c = vec3f(select(0.0, 1.0, id == 0.0), fract(id * 0.37), fract(id * 0.71));
    }
    default: {}
  }
  textureStore(out_w, px, vec4f(c, 1.0));
}

// ---- load time: cook the distance volume, place the probes ------------------------------------------------------

/** Fills one level of the objects' distance clipmap (flags.y: 0 fine, 1 coarse) from the analytic field. */
@compute @workgroup_size(4, 4, 4)
fn cook(@builtin(global_invocation_id) gid: vec3u) {
  let coarse = F.flags.y == 1u;
  let dims = select(F.vol_n.xyz, F.vol1_n.xyz, coarse);
  if (any(gid >= dims)) { return; }
  let o = select(F.vol_o, F.vol1_o, coarse);
  textureStore(vol_w, gid, vec4f(objects(o.xyz + (vec3f(gid) + 0.5) * o.w), 0.0, 0.0, 1.0));
}

@group(1) @binding(40) var hm_w: texture_storage_2d<rgba32float, write>;
@group(1) @binding(42) var sh_w: texture_storage_2d<rg32float, write>;

/** Fills the shadow-height map for the current sun, one band of rows per submission (F.rf.w holds
 *  the band's first row in its high 16 bits). For each texel: march toward the sun over the height
 *  map, keeping the largest h(q) − tan(elevation)·distance, the height a point there must clear to
 *  see the sun's centre, and the distance at which it occurs. Starts two texels out, so a texel
 *  doesn't shadow itself; stops when no remaining terrain could rise above the current best. */
@compute @workgroup_size(8, 8)
fn cook_shadow(@builtin(global_invocation_id) gid0: vec3u) {
  let gid = gid0.xy + vec2u(0u, F.rf.w >> 16u);
  if (any(gid >= F.hm_n.xy)) { return; }
  let x0 = F.hm_o.xy + vec2f(gid) * F.hm_o.z;
  let sd = normalize(F.sun.xz);
  let te = F.sun.y / length(F.sun.xz);
  var best = -1e9;
  var bd = 0.0;
  var dist = 2.0 * F.hm_o.z;
  let ext = F.hm_o.xy + vec2f(F.hm_n.xy - 1u) * F.hm_o.z;
  for (var i = 0u; i < 512u; i++) {
    let q = x0 + sd * dist;
    if (any(q < F.hm_o.xy) || any(q > ext)) { break; }
    let g = (q - F.hm_o.xy) / F.hm_o.z;
    let h = textureSampleLevel(hm_t, lin, (g + 0.5) / vec2f(textureDimensions(hm_t)), 0.0).r;
    let hs = h - te * dist;
    if (hs > best) { best = hs; bd = dist; }
    if (ground_max() - te * dist < best || dist > F.lp.y) { break; }
    dist += max(F.hm_o.z, dist * 0.02);
  }
  textureStore(sh_w, vec2i(gid), vec4f(best, bd, 0.0, 0.0));
}

/** Fills the terrain height map: height and gradient at each texel's corner-aligned position. */
@compute @workgroup_size(8, 8)
fn cook_height(@builtin(global_invocation_id) gid: vec3u) {
  if (any(gid.xy >= F.hm_n.xy)) { return; }
  let p = F.hm_o.xy + vec2f(gid.xy) * F.hm_o.z;
  textureStore(hm_w, vec2i(gid.xy), vec4f(terrain(p.x, p.y), 1.0));
}

/** A fixed amount of arithmetic, timed at the start and end of each scene: a contention gauge.
 *  On a quiet GPU its time is constant; when other work shares the GPU, it stretches. */
@compute @workgroup_size(64)
fn canary(@builtin(global_invocation_id) gid: vec3u) {
  var x = f32(gid.x) * 1e-6;
  var y = 1.0;
  for (var i = 0u; i < 4096u; i++) { x = fma(x, 0.999, 0.001); y = fma(y, x, 0.5) * 0.5; }
  if (y == 12345.0) { stat(63u, 1u); }
}

fn sdf_grad(p: vec3f, e: f32) -> vec3f {
  let k = vec2f(1.0, -1.0);
  return k.xyy * sdf(p + k.xyy * e) + k.yyx * sdf(p + k.yyx * e) + k.yxy * sdf(p + k.yxy * e) + k.xxx * sdf(p + k.xxx * e);
}

/** Probe placement. DDGI mode (flags.y = 0): push each probe out of nearby geometry along the field's
 *  gradient (within 0.45 of a cell), switch it off if it's still inside, or if it's too far from any
 *  surface to ever be interpolated (> 2.1 cells). Naive mode (flags.y = 1): every probe on its grid
 *  point, all on. */
@compute @workgroup_size(64)
fn relocate(@builtin(global_invocation_id) gid: vec3u) {
  let i = gid.x;
  if (i >= F.pg_n.w) { return; }
  if (F.flags.y == 1u) { probes_rw[i] = vec4f(0.0, 0.0, 0.0, 1.0); stat(S_PROBE_ACTIVE, 1u); return; }
  let g = probe_grid_pos(i);
  let sp = F.pg_s.xyz;
  let s = min(sp.x, min(sp.y, sp.z));
  let smax = max(sp.x, max(sp.y, sp.z));
  let want = 0.3 * s;
  var off = vec3f(0.0);
  var d = sdf(g);
  for (var k = 0; k < 8 && d < want; k++) {
    let gr = sdf_grad(g + off, 0.02 * s);
    off = clamp(off + normalize(gr + vec3f(0.0, 1e-6, 0.0)) * (want - d), -0.45 * sp, 0.45 * sp);
    d = sdf(g + off);
  }
  var on = 1.0;
  if (d < 0.05 * s) { on = 0.0; stat(S_PROBE_SOLID, 1u); }
  else if (d > 2.1 * smax) { on = 0.0; stat(S_PROBE_FAR, 1u); }
  else { stat(S_PROBE_ACTIVE, 1u); }
  probes_rw[i] = vec4f(off, on);
}
