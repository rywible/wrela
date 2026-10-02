// The whole frame in one compute kernel: terrain, creatures, shadows, shading. No triangles.
// Appended to common.wgsl + field.wgsl.

@group(0) @binding(2) var<storage, read> tiles: array<u32>;
@group(0) @binding(3) var<storage, read> lbins: array<u32>;
@group(0) @binding(4) var out_tex: texture_storage_2d<rgba16float, write>;
@group(0) @binding(5) var<storage, read_write> stats: array<atomic<u32>, 32>;

// Variants are pipeline-overridable constants, so each one compiles to its own specialized kernel.
override CREATURES: bool = true;
override SHADOWS: bool = true;
override STATS: bool = false;
override VIEW: u32 = 0u;                // 0 shaded, 1 primary-step heat map, 2 shadow-step heat map
override DISPLACED: bool = false;       // trace the displaced surface (true silhouettes), not just shade it
override RELAX: f32 = 1.0;              // over-relaxation factor ω (1 = plain sphere tracing)
override STEP_SCALE: f32 = 1.0;         // < 1 for the reference march
override EPS_SCALE: f32 = 0.25;         // hit tolerance, in pixel footprints (0.5 fails the reference check)
override MAX_STEPS: u32 = 128u;
override SHADOW_STEPS: u32 = 48u;
override TERRAIN_STEPS: u32 = 160u;

const SOFT: f32 = 24.0;                 // soft-shadow sharpness (penumbra ∝ distance / SOFT)
const SKY = vec3f(0.42, 0.55, 0.75);    // spike 01's clear colour

struct Counters {
  steps: u32, parts: u32, detail: u32, caps: u32, relax: u32, misses: u32, marches: u32,
  sh_marches: u32, sh_steps: u32, sh_caps: u32, t_steps: u32, entries: u32,
}

// ---- terrain: spike 01's height function, traced as a heightfield --------------------------------

const T_HALF: f32 = 80.0;               // spike 01's 160 m square
const T_YMIN: f32 = -1.0;
const T_YMAX: f32 = 1.0;                // |h| ≤ 0.98
const T_SLOPE: f32 = 0.27;              // sup |∇h| ≤ √(0.207² + 0.167²) = 0.266

fn terrain_h(x: f32, z: f32) -> f32 {
  return 0.6 * sin(0.11 * x) * cos(0.09 * z)
       + 0.3 * sin(0.23 * x + 1.3) * sin(0.19 * z + 0.4)
       + 0.08 * sin(0.9 * x) * cos(0.7 * z);
}

fn terrain_n(x: f32, z: f32) -> vec3f {
  let hx = 0.066 * cos(0.11 * x) * cos(0.09 * z)
         + 0.069 * cos(0.23 * x + 1.3) * sin(0.19 * z + 0.4)
         + 0.072 * cos(0.9 * x) * cos(0.7 * z);
  let hz = -0.054 * sin(0.11 * x) * sin(0.09 * z)
         + 0.057 * sin(0.23 * x + 1.3) * cos(0.19 * z + 0.4)
         - 0.056 * sin(0.9 * x) * sin(0.7 * z);
  return normalize(vec3f(-hx, 1.0, -hz));
}

fn safe_inv(x: f32) -> f32 { return 1.0 / select(x, select(-1e-9, 1e-9, x >= 0.0), abs(x) < 1e-9); }

/** Heightfield tracing with a directional Lipschitz bound: along the ray, the gap y − h(x, z)
 *  shrinks at most at rate −d.y + T_SLOPE·|d.xz|, so stepping gap / rate never crosses the surface.
 *  At grazing angles that's several times longer than a 3D sphere-tracing step. */
fn terrain_trace(o: vec3f, d: vec3f, c: ptr<function, Counters>) -> f32 {
  let inv = vec3f(safe_inv(d.x), safe_inv(d.y), safe_inv(d.z));
  let lo = (vec3f(-T_HALF, T_YMIN, -T_HALF) - o) * inv;
  let hi = (vec3f(T_HALF, T_YMAX, T_HALF) - o) * inv;
  let t0 = max(max(min(lo.x, hi.x), min(lo.y, hi.y)), max(min(lo.z, hi.z), 0.0));
  let t1 = min(min(max(lo.x, hi.x), max(lo.y, hi.y)), max(lo.z, hi.z));
  if (t1 < t0) { return INF; }
  let rate = -d.y + T_SLOPE * length(d.xz);
  if (rate <= 0.0) { return INF; }
  var t = t0;
  var t_prev = t0;
  var gap_prev = INF;
  for (var i = 0u; i < TERRAIN_STEPS; i++) {
    (*c).t_steps += 1u;
    let p = o + d * t;
    let gap = p.y - terrain_h(p.x, p.z);
    if (gap < EPS_SCALE * t * F.eye.w) {
      // Secant through the last two samples: the hit lands on the surface, not up to a
      // footprint above it, which at grazing angles is a long way along the ray.
      if (gap_prev < INF && gap_prev > gap) { t += gap * (t - t_prev) / (gap_prev - gap); }
      return t;
    }
    t_prev = t;
    gap_prev = gap;
    t += gap / rate * STEP_SCALE;
    if (t > t1) { return INF; }
  }
  return t;
}

// ---- creatures ------------------------------------------------------------------------------------

/** Sphere-traces one instance's masked field from t_start to t_end. INF if nothing is hit. */
fn march(base: u32, mask: u32, o: vec3f, d: vec3f, t_start: f32, t_end: f32, c: ptr<function, Counters>) -> f32 {
  let misc = iv(base, I_MISC);
  let disp = misc.y * 1.875;              // the displacement's bound
  let np = countOneBits(mask);
  (*c).marches += 1u;
  var t = t_start;
  var mode = select(F_BASE, F_BOUND, DISPLACED);
  var sc = STEP_SCALE;
  var w = RELAX;
  var t_prev = t;
  var r_prev = 0.0;
  for (var i = 0u; i < MAX_STEPS; i++) {
    (*c).steps += 1u;
    (*c).parts += np;
    if (mode == F_FULL) { (*c).detail += 1u; }
    let p = o + d * t;
    let fp = t * F.eye.w;
    let eps = max(EPS_SCALE * fp, 1e-4);
    var dist = field(base, mask, p, fp, mode);
    if (mode == F_BOUND && dist < disp + eps) {
      // Near the surface: switch to the displaced field, with steps shrunk by its slope.
      mode = F_FULL;
      sc = STEP_SCALE / (1.0 + fbm_lipschitz(fp, misc));
      dist = field(base, mask, p, fp, mode);
      (*c).detail += 1u;
      (*c).parts += np;
    }
    let r = dist * sc;
    if (w > 1.0 && i > 0u && r + r_prev < t - t_prev) {
      // Over-relaxed step left a gap between the unbounding spheres: go back to the safe step.
      t = t_prev + r_prev;
      w = 1.0;
      (*c).relax += 1u;
      continue;
    }
    if (dist < eps) { return t + dist; }   // one more step: closer to the surface, no extra evaluation
    t_prev = t;
    r_prev = r;
    t += r * w;
    if (t > t_end) { (*c).misses += 1u; return INF; }
  }
  (*c).caps += 1u;
  return INF;
}

/** Soft shadow from one instance: the usual min(SOFT·h/t) penumbra estimate. */
fn shadow_march(base: u32, mask: u32, o: vec3f, d: vec3f, t0: f32, t1: f32, c: ptr<function, Counters>) -> f32 {
  (*c).sh_marches += 1u;
  var res = 1.0;
  var t = t0;
  var ph = 1e10;
  for (var i = 0u; i < SHADOW_STEPS; i++) {
    (*c).sh_steps += 1u;
    let h = field(base, mask, o + d * t, 0.0, select(F_BASE, F_BOUND, DISPLACED));
    if (h < 1e-4) { return 0.0; }
    // Closest approach between this sample and the last (Quílez's improved soft shadows), so the
    // penumbra depends less on where the steps happen to land.
    let y = h * h / (2.0 * ph);
    let dd = sqrt(max(h * h - y * y, 0.0));
    res = min(res, SOFT * dd / max(t - y, 1e-3));
    if (res < 0.005) { return 0.0; }
    ph = h;
    t += max(h * STEP_SCALE, 0.002);
    if (t > t1) { return res; }
  }
  (*c).sh_caps += 1u;
  return res;
}

/** Visibility of the sun from p. Shadow rays are parallel, so a ray stays in one light-grid cell,
 *  and that cell's list holds every candidate occluder. */
fn shadow(p: vec3f, n: vec3f, eps: f32, c: ptr<function, Counters>) -> f32 {
  if (!SHADOWS || !CREATURES) { return 1.0; }
  let o = p + n * (0.02 + 2.0 * eps);
  let rel = o - F.lc.xyz;
  let g = f32(F.grid.x);
  let e = F.lc.w;
  let l = (vec2f(dot(rel, F.lr.xyz), dot(rel, F.lu.xyz)) + e) / (2.0 * e) * g;
  if (any(l < vec2f(0.0)) || any(l >= vec2f(g))) { return 1.0; }
  let cb = (u32(l.y) * F.grid.x + u32(l.x)) * BIN_WORDS;
  let cnt = lbins[cb];
  let d = F.sun.xyz;
  var res = 1.0;
  for (var k = 0u; k < cnt; k++) {
    let base = lbins[cb + 1u + 2u * k] * STRIDE;
    var m = lbins[cb + 2u + 2u * k];
    var pm = 0u;
    var t0 = INF;
    var t1 = 0.0;
    loop {
      if (m == 0u) { break; }
      let b = firstTrailingBit(m);
      m &= m - 1u;
      let r = part_interval(base, b, o, d);
      if (r.y > 0.0 && r.x <= r.y) {
        pm |= 1u << b;
        t0 = min(t0, r.x);
        t1 = max(t1, r.y);
      }
    }
    if (pm == 0u) { continue; }
    res = min(res, shadow_march(base, pm, o, d, max(t0, 0.0), t1, c));
    if (res <= 0.0) { break; }
  }
  return res;
}

// ---- shading (spike 01's lighting, with the shadow factor passed in) -----------------------------------

const HIDE = vec3f(0.148, 0.089, 0.047);
const HIDE_DARK = vec3f(0.060, 0.037, 0.021);
const HOOF = vec3f(0.0130, 0.0090, 0.0070);

fn tonemap(x: vec3f) -> vec3f {
  let c = x * 0.8;
  return clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), vec3f(0.0), vec3f(1.0));
}

fn light(albedo: vec3f, rough: f32, n: vec3f, world: vec3f, sh: f32) -> vec3f {
  let l = F.sun.xyz;
  let v = normalize(F.eye.xyz - world);
  let h = normalize(l + v);
  let ndl = max(dot(n, l), 0.0);
  let a = max(rough * rough, 0.02);
  let sp = 2.0 / (a * a) - 2.0;
  let spec = pow(max(dot(n, h), 0.0), sp) * (sp + 8.0) / 25.13 * 0.04;
  let sky = mix(vec3f(0.30, 0.26, 0.20), vec3f(0.42, 0.55, 0.75), 0.5 + 0.5 * n.y);
  let sun = vec3f(3.2, 2.9, 2.5);
  return tonemap(albedo * (sky * 0.55 + sun * ndl * sh) + sun * spec * ndl * sh);
}

fn dapple(p: vec3f, fp: f32, seed: vec4f) -> f32 {
  let f = seed.w;
  let q = (p + seed.xyz) * f;
  let n = noise_d(q) + 0.5 * noise_d(q * 2.0 + 31.0);
  let edge = max(0.08, fp * f * 2.0);
  return smoothstep(0.25 - edge, 0.25 + edge, n);
}

fn heat(x: f32) -> vec3f {
  let t = clamp(x, 0.0, 1.0);
  return clamp(vec3f(1.5 * t, 1.5 * t * t, 0.5 - t) + vec3f(0.0, 0.0, 0.2) * (1.0 - t), vec3f(0.0), vec3f(1.0));
}

// ---- the kernel ---------------------------------------------------------------------------------------

@compute @workgroup_size(8, 8)
fn trace(@builtin(global_invocation_id) gid: vec3u, @builtin(workgroup_id) wg: vec3u) {
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  var c = Counters(0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u);
  let u = (f32(gid.x) + 0.5) / f32(F.dims.x) * 2.0 - 1.0;
  let v = 1.0 - (f32(gid.y) + 0.5) / f32(F.dims.y) * 2.0;
  let o = F.eye.xyz;
  let d = normalize(F.cf.xyz + F.cr.xyz * u + F.cu.xyz * v);

  // Terrain first: its distance bounds every creature march.
  let t_terrain = terrain_trace(o, d, &c);
  var best = t_terrain;
  var hit_base = 0xFFFFFFFFu;
  var hit_mask = 0u;

  if (CREATURES) {
    let tb = (wg.y * F.dims.z + wg.x) * BIN_WORDS;
    let n = tiles[tb];
    c.entries = n;
    for (var e = 0u; e < n; e++) {
      let base = tiles[tb + 1u + 2u * e] * STRIDE;
      var m = tiles[tb + 2u + 2u * e];
      // Exact ray intervals against each candidate part's inflated bound: the per-ray PartMask.
      var pm = 0u;
      var t0 = INF;
      var t1 = 0.0;
      loop {
        if (m == 0u) { break; }
        let b = firstTrailingBit(m);
        m &= m - 1u;
        let r = part_interval(base, b, o, d);
        if (r.y > 0.0 && r.x <= r.y && r.x < best) {
          pm |= 1u << b;
          t0 = min(t0, r.x);
          t1 = max(t1, r.y);
        }
      }
      if (pm == 0u) { continue; }
      let t = march(base, pm, o, d, max(t0, 0.0), min(t1, best), &c);
      if (t < best) {
        best = t;
        hit_base = base;
        hit_mask = pm;
      }
    }
  }

  var color = SKY;
  let eps = EPS_SCALE * best * F.eye.w;
  if (hit_base != 0xFFFFFFFFu) {
    let p = o + d * best;
    let fp = best * F.eye.w * 1.5;          // ~ spike 01's length(fwidth(rest))
    let s = field_g(hit_base, hit_mask, p, fp);
    let n = normalize(s.g);
    let seed = iv(hit_base, I_SEED);
    var hide = HIDE;
    if (s.ch.x > 0.0) { hide = mix(HIDE, HIDE_DARK, dapple(to_rest(hit_base, I_TINV, p), fp, seed) * s.ch.x); }
    let albedo = mix(hide, HOOF, s.ch.y);
    var sh = 1.0;
    if (dot(n, F.sun.xyz) > 0.0) { sh = shadow(p, n, eps, &c); }
    color = light(albedo, mix(0.7, 0.35, s.ch.y), n, p, sh);
  } else if (best < INF) {
    let p = o + d * best;
    let n = terrain_n(p.x, p.z);
    let g = noise_d(vec3f(p.x, 0.0, p.z) * 0.35) * 0.5 + 0.5;
    let albedo = mix(vec3f(0.035, 0.060, 0.012), vec3f(0.075, 0.085, 0.020), g);
    var sh = 1.0;
    if (dot(n, F.sun.xyz) > 0.0) { sh = shadow(p, n, eps, &c); }
    color = light(albedo, 0.9, n, p, sh);
  }

  if (VIEW == 1u && c.marches > 0u) { color = heat(f32(c.steps + c.detail) / 64.0); }
  if (VIEW == 2u && c.sh_marches > 0u) { color = heat(f32(c.sh_steps) / 48.0); }
  textureStore(out_tex, vec2i(gid.xy), vec4f(color, 1.0));

  if (STATS) {
    if (hit_base != 0xFFFFFFFFu) { atomicAdd(&stats[S_CREATURE_PX], 1u); atomicAdd(&stats[S_HIT_STEPS], c.steps); }
    else if (best < INF) { atomicAdd(&stats[S_TERRAIN_PX], 1u); }
    else { atomicAdd(&stats[S_SKY_PX], 1u); }
    atomicAdd(&stats[S_MARCHES], c.marches);
    atomicAdd(&stats[S_STEPS], c.steps);
    atomicAdd(&stats[S_MISSES], c.misses);
    atomicAdd(&stats[S_CAPS], c.caps);
    if (c.sh_marches > 0u) { atomicAdd(&stats[S_SHADOW_PX], 1u); }
    atomicAdd(&stats[S_SHADOW_MARCHES], c.sh_marches);
    atomicAdd(&stats[S_SHADOW_STEPS], c.sh_steps);
    atomicAdd(&stats[S_SHADOW_CAPS], c.sh_caps);
    atomicAdd(&stats[S_TERRAIN_STEPS], c.t_steps);
    atomicAdd(&stats[S_PART_EVALS], c.parts);
    atomicAdd(&stats[S_DETAIL_EVALS], c.detail);
    atomicAdd(&stats[S_TILE_ENTRIES], c.entries);
    atomicAdd(&stats[S_RELAX_FAILS], c.relax);
  }
}
