// Lighting (spike 05's slice, reported separately): materials, sun with a soft shadow marched through
// the cache, sky ambient with cache ambient occlusion. Appended to common + field + cache.

@group(0) @binding(5) var<storage, read_write> lstats: array<atomic<u32>, 64>;
@group(1) @binding(0) var gbufT: texture_2d<f32>;
@group(1) @binding(1) var depthT: texture_2d<f32>;
@group(1) @binding(2) var hdr: texture_storage_2d<rgba16float, write>;

override SHADOWS: bool = true;
override LSTATS: bool = false;

const SOFT: f32 = 10.0;
const SHADOW_STEPS: u32 = 56u;
const S_SH_STEPS: u32 = 8u;
const S_SH_RAYS: u32 = 9u;

const SUN_COL = vec3f(1.0, 0.84, 0.66) * 3.4;

/** Soft sun shadow through the cache, from camera-relative p. */
fn cache_shadow(p: vec3f, l: vec3f, t0: f32, tmax: f32) -> vec2f {
  var res = 1.0;
  var t = t0;
  var n = 0u;
  for (var i = 0u; i < SHADOW_STEPS; i++) {
    n += 1u;
    let c = cache_at(p + l * t, l);
    if (c.kind == 2u) { break; }
    var step = c.d;
    if (c.kind == 1u) {
      if (c.d < 0.0) { return vec2f(0.0, f32(n)); }
      step = max(c.d, c.exit + 1e-3 * F.lv[c.level].rel.w);
    } else {
      if (c.d < 1e-3 * t) { return vec2f(0.0, f32(n)); }
    }
    res = min(res, SOFT * max(c.est, 0.0) / t);
    if (res < 0.01) { return vec2f(0.0, f32(n)); }
    t += max(step, 0.02 * F.lv[c.level].bnd.x);
    if (t > tmax) { break; }
  }
  return vec2f(res, f32(n));
}

fn cache_ao(p: vec3f, n: vec3f, cell: f32) -> f32 {
  var occ = 0.0;
  var w = 0.5;
  var h = 2.0 * cell;
  for (var i = 0u; i < 4u; i++) {
    let d = cache_d(p + n * h);
    occ += w * clamp((h - d) / h, 0.0, 1.0);
    w *= 0.6;
    h *= 2.2;
  }
  return clamp(1.0 - 1.3 * occ, 0.0, 1.0);
}

fn smooth_noise2(q: vec2f, fp: f32, wl: f32, seed: u32) -> f32 {
  return noise2(q / wl, seed).x * octave_weight(fp, wl);
}

fn terrain_albedo(pc: vec3f, n: vec3f, fp: f32) -> vec3f {
  let slope = 1.0 - n.y;
  let n1 = smooth_noise2(pc.xz, fp, 83.0, 71u) * 0.6 + smooth_noise2(pc.xz, fp, 19.0, 72u) * 0.4;
  let n2 = smooth_noise2(pc.xz, fp, 3.7, 73u) * 0.6 + smooth_noise2(pc.xz, fp, 0.9, 74u) * 0.4;
  let n3 = smooth_noise2(pc.xz, fp, 0.23, 75u);
  let lush = vec3f(0.060, 0.085, 0.030);
  let meadow = vec3f(0.115, 0.125, 0.045);
  let dry = vec3f(0.165, 0.145, 0.075);
  let soil = vec3f(0.115, 0.090, 0.062);
  let rock = mix(vec3f(0.17, 0.16, 0.15), vec3f(0.30, 0.285, 0.26), clamp(0.5 + 0.5 * n2 + 0.3 * n3, 0.0, 1.0));
  let snow = vec3f(0.78, 0.81, 0.86);
  var c = mix(lush, meadow, smoothstep(-0.4, 0.3, n1));
  c = mix(c, dry, smoothstep(0.25, 0.7, n1 + 0.35 * n2) * 0.8);
  // worn soil and small stones in patches
  c = mix(c, soil, smoothstep(0.35, 0.65, n2 + 0.5 * slope) * 0.7);
  c = mix(c, rock * 0.6, smoothstep(0.68, 0.85, n3 + 0.25 * n2 + 0.3 * slope) * 0.45);
  // rock on steep ground, and more of it high up
  let alt = pc.y + 90.0 * n1;
  let rockiness = max(smoothstep(0.30, 0.48, slope + 0.08 * n2), smoothstep(900.0, 1300.0, alt) * 0.85);
  c = mix(c, rock, rockiness);
  let snowy = smoothstep(1180.0, 1380.0, alt) * smoothstep(0.55, 0.3, slope + 0.1 * n2);
  c = mix(c, snow, snowy);
  return c * (0.88 + 0.12 * n3);
}

/** Masonry in the nearest structure's own coordinates: coursed blocks with per-block tint. */
fn stone_albedo(pc: vec3f, n: vec3f, fp: f32) -> vec3f {
  var best = 1e9;
  var u = pc.x + pc.z;
  let nr = u32(ruins[0].x);
  for (var i = 0u; i < nr; i++) {
    let a = ruins[1u + 3u * i];
    let b = ruins[2u + 3u * i];
    let q = pc.xz - a.xz;
    let dd = dot(q, q);
    if (dd < best) {
      best = dd;
      let ql = vec2f(b.x * q.x - b.y * q.y, b.y * q.x + b.x * q.y);
      if (u32(a.w) == 0u) { u = atan2(ql.y, ql.x) * b.z; }
      else { u = select(ql.x, ql.y, abs(n.x * b.x + n.z * b.y) > 0.7); }
    }
  }
  let course = floor(pc.y / 0.36);
  let bu = (u + fract(course * 0.37) * 0.75) / 0.75;
  let cell = vec2f(floor(bu), course);
  let h = f32(hash2i(vec2i(cell), 83u)) / 4294967295.0;
  let fy = fract(pc.y / 0.36);
  let fu = fract(bu);
  let jw = octave_weight(fp, 0.36);
  let joint = max(1.0 - smoothstep(0.0, 0.07, min(fy, 1.0 - fy)), 1.0 - smoothstep(0.0, 0.05, min(fu, 1.0 - fu))) * jw;
  let n2 = noise3(pc * 2.3, 82u) * octave_weight(fp, 0.45);
  let base = mix(vec3f(0.36, 0.32, 0.27), vec3f(0.46, 0.42, 0.35), h) * (0.9 + 0.1 * n2);
  return mix(base, vec3f(0.17, 0.15, 0.13), joint * 0.8);
}

fn heat(x: f32) -> vec3f {
  let t = clamp(x, 0.0, 1.0);
  return clamp(vec3f(1.5 * t, 1.5 * t * t, 0.5 - t) + vec3f(0.0, 0.0, 0.2) * (1.0 - t), vec3f(0.0), vec3f(1.0));
}

fn level_color(L: u32) -> vec3f {
  let k = f32(L) * 0.61803;
  return 0.25 + 0.6 * abs(fract(vec3f(k, k + 0.33, k + 0.66)) * 2.0 - 1.0);
}

@compute @workgroup_size(8, 8)
fn light(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let px = vec2i(gid.xy);
  let t = textureLoad(depthT, px, 0).r;
  let g = textureLoad(gbufT, px, 0);
  if (F.dbg.x == 1u || F.dbg.x == 3u) {
    textureStore(hdr, px, vec4f(heat(fract(g.w) * 1000.0 / select(160.0, 48.0, F.dbg.x == 3u)), 1.0));
    return;
  }
  if (t >= INF) { textureStore(hdr, px, vec4f(0.0)); return; }
  if (F.dbg.x == 2u) {
    let n = oct_decode(g.xy);
    textureStore(hdr, px, vec4f(level_color(u32(g.w)) * (0.35 + 0.65 * max(dot(n, F.sun.xyz), 0.0)), 1.0));
    return;
  }
  let dir = ray_dir(vec2f(gid.xy) + 0.5);
  let rel = dir * t;
  let pc = F.cam.xyz + rel - F.shift.xyz;
  let n = oct_decode(g.xy);
  let fp = t * F.cam.w;
  let L = min(u32(g.w), F.dims.z - 1u);
  let cell = F.lv[L].bnd.x;
  var albedo = terrain_albedo(pc, n, fp);
  var rough = 0.9;
  if (g.z > 0.5) { albedo = stone_albedo(pc, n, fp); rough = 0.7; }

  let l = F.sun.xyz;
  let ndl = max(dot(n, l), 0.0);
  var sh = 1.0;
  if (SHADOWS && ndl > 0.0) {
    let off = 2.0 * cell + 2.0 * F.lv[L].bnd.y;
    let s = cache_shadow(rel + n * off, l, off, 4000.0);
    sh = s.x;
    if (LSTATS) { atomicAdd(&lstats[S_SH_STEPS], u32(s.y)); atomicAdd(&lstats[S_SH_RAYS], 1u); }
  }
  let ao = cache_ao(rel + n * F.lv[L].bnd.y, n, cell);
  let sky = mix(vec3f(0.30, 0.30, 0.28), vec3f(0.34, 0.48, 0.72), 0.5 + 0.5 * n.y) * 0.75;
  let bounce = vec3f(0.30, 0.24, 0.16) * 0.22 * clamp(0.5 - 0.5 * n.y, 0.0, 1.0);
  let v = -dir;
  let hv = normalize(l + v);
  let a = rough * rough;
  let sp = 2.0 / (a * a) - 2.0;
  let spec = pow(max(dot(n, hv), 0.0), sp) * (sp + 8.0) / 25.13 * 0.04;
  let col = albedo * (SUN_COL * ndl * sh + (sky + bounce) * ao) + SUN_COL * spec * ndl * sh;
  textureStore(hdr, px, vec4f(col, 1.0));
}
