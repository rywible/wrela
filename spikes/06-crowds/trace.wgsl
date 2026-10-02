// The frame in one compute kernel: the static world (bricks, or analytic), the tile's moving things,
// one sun shadow ray through both, and shading. No triangles.
// Appended to common.wgsl + world.wgsl + creature.wgsl + cache.wgsl.

@group(0) @binding(1) var<storage, read> inst: array<vec4f>;
@group(0) @binding(2) var<storage, read> tiles: array<u32>;
@group(0) @binding(3) var<storage, read> lbins: array<u32>;
@group(0) @binding(4) var<storage, read> bmap: array<u32>;
@group(0) @binding(5) var atlas: texture_3d<f32>;
@group(0) @binding(6) var samp: sampler;
@group(0) @binding(7) var<storage, read> edits: array<vec4f>;
@group(0) @binding(8) var out_tex: texture_storage_2d<rgba16float, write>;
@group(0) @binding(9) var<storage, read_write> stats: array<atomic<u32>, 32>;
@group(0) @binding(10) var<storage, read> egrid: array<vec2u>;   // per 4 m cell: (first, count) into eidx
@group(0) @binding(11) var<storage, read> eidx: array<u32>;      // log entries per cell, in log order

// Variants are pipeline-overridable constants: each compiles to its own specialized kernel.
override CREATURES: bool = true;
override SHADOWS: bool = true;            // sun shadows at all
override CREATURE_SHADOWS: bool = true;   // moving things as occluders
override WORLD_ANALYTIC: bool = false;    // march base + every edit directly instead of the bricks
override REF: bool = false;               // brute-force moving things: every instance, every part
override STATS: bool = false;
override VIEW: u32 = 0u;                  // 0 shaded; heat maps: 1 world steps, 2 creature steps, 3 shadow steps
override SHADE: bool = true;              // false: no materials, AO or detail on the world (diagnostic)
// Moving things: step fraction, hit tolerance (pixel footprints), step caps.
override C_STEP: f32 = 1.0;
override C_EPS: f32 = 0.25;
override MAX_STEPS: u32 = 96u;            // per creature march
override SHADOW_STEPS: u32 = 48u;         // per creature shadow march
// The world: brick step fraction, analytic step fraction, hit tolerance, step caps.
override W_STEP: f32 = 0.9;               // brick march: a fraction of the trilinear distance
override WA_STEP: f32 = 1.0;              // analytic march (< 1 for the reference)
override W_EPS: f32 = 0.25;
override WORLD_STEPS: u32 = 192u;
override WSHADOW_STEPS: u32 = 96u;
override TERRAIN_STEPS: u32 = 160u;       // the heightfield beyond the region

// ---- the terrain beyond the brick region: spike 02's directional-Lipschitz heightfield trace ------

const T_HALF: f32 = 900.0;
const T_YMIN: f32 = -3.0;
const T_YMAX: f32 = 34.0;
const T_SLOPE: f32 = 0.5;     // bound on |∇h| outside the plaza; main.js checks it numerically

fn terrain_trace(o: vec3f, d: vec3f, ts: f32, te: f32, c: ptr<function, Counters>) -> f32 {
  let span = ray_box(o, d, vec3f(-T_HALF, T_YMIN, -T_HALF), vec3f(T_HALF, T_YMAX, T_HALF));
  let t0 = max(max(span.x, ts), 0.0);
  let t1 = min(span.y, te);
  if (t1 <= t0) { return INF; }
  let rate = -d.y + T_SLOPE * length(d.xz);
  if (rate <= 0.0) { return INF; }
  var t = t0;
  var t_prev = t0;
  var gap_prev = INF;
  for (var i = 0u; i < TERRAIN_STEPS; i++) {
    (*c).t_steps += 1u;
    let p = o + d * t;
    let gap = p.y - terrain_h(p.x, p.z);
    if (gap < W_EPS * t * F.eye.w) {
      if (gap_prev < INF && gap_prev > gap) { t += gap * (t - t_prev) / (gap_prev - gap); }
      return t;
    }
    t_prev = t;
    gap_prev = gap;
    t += gap / rate * WA_STEP;
    if (t > t1) { return INF; }
  }
  return t;
}

// ---- the analytic world (variants and reference) ---------------------------------------------------
// Base plus the log, evaluated directly. Each 2 m cell lists the log entries that can change the
// field anywhere it's evaluated in that cell (outside a surface, or up to 0.5 m inside it, for
// normals and AO), given that the result is clamped to AN_CLAMP (edits.js `listReach`). The clamp
// keeps a skipped, distant addition from making a step unsafe; at 2 m it can't change a penumbra,
// because shadow rays leave the 32 m-high region before SOFT · 2 / t falls below 1.

const EG: f32 = 2.0;
const EGX: i32 = 48;
const EGY: i32 = 20;
const EGZ: i32 = 48;
const AN_CLAMP: f32 = 2.0;

fn edit_cell(p: vec3f) -> vec2u {
  let g = clamp(vec3i(floor((p - RMIN) / EG)), vec3i(0), vec3i(EGX - 1, EGY - 1, EGZ - 1));
  return egrid[u32((g.y * EGZ + g.z) * EGX + g.x)];
}

fn world_d(p: vec3f) -> f32 {
  var d = base_field(p);
  let cl = edit_cell(p);
  for (var j = 0u; j < cl.y; j++) {
    let i = eidx[cl.x + j];
    if (i >= F.grid.w) { break; }
    d = edit_apply_d(p, d, i);
  }
  return min(d, AN_CLAMP);
}

/** Distance and the disturbed channel (once per pixel, for shading). */
fn world_all(p: vec3f) -> vec2f {
  var dg = vec2f(base_field(p), 0.0);
  let cl = edit_cell(p);
  for (var j = 0u; j < cl.y; j++) {
    let i = eidx[cl.x + j];
    if (i >= F.grid.w) { break; }
    dg = edit_apply(p, dg, i);
  }
  return vec2f(min(dg.x, AN_CLAMP), dg.y);
}

fn march_analytic(o: vec3f, d: vec3f, t0: f32, t1: f32, c: ptr<function, Counters>) -> f32 {
  var t = t0;
  var t_prev = t0;
  var d_prev = INF;
  for (var i = 0u; i < WORLD_STEPS; i++) {
    (*c).w_steps += 1u;
    let dist = world_d(o + d * t);
    let eps = max(W_EPS * t * F.eye.w, 2e-4);
    if (dist < eps) { return refine_hit(t, dist, t_prev, d_prev, eps); }
    t_prev = t;
    d_prev = dist;
    t += dist * WA_STEP;
    if (t > t1) { return INF; }
  }
  (*c).w_caps += 1u;
  return t;
}

fn shadow_analytic(o: vec3f, c: ptr<function, Counters>) -> f32 {
  let d = F.sun.xyz;
  let span = ray_box(o, d, RMIN, RMAX);
  if (span.y <= max(span.x, 0.0)) { return 1.0; }
  (*c).ws_marches += 1u;
  var t = max(span.x, 0.0);
  var res = 1.0;
  var ph = 1e10;
  for (var i = 0u; i < WSHADOW_STEPS; i++) {
    (*c).ws_steps += 1u;
    let h = world_d(o + d * t);
    if (h < 1e-3) { return 0.0; }
    let y = h * h / (2.0 * ph);
    let dd = sqrt(max(h * h - y * y, 0.0));
    res = min(res, SOFT * dd / max(t - y, 1e-3));
    if (res < 0.004) { return 0.0; }
    ph = h;
    t += max(h * WA_STEP, 0.004 * WA_STEP);
    if (t > span.y) { return res; }
  }
  (*c).ws_caps += 1u;
  return res;
}

fn in_region(p: vec3f) -> bool { let q = p - RMIN; return all(q >= vec3f(0.0)) && all(q < RMAX - RMIN); }

/** Distance of the world at p, from whichever representation this variant uses. */
fn wd(p: vec3f) -> f32 {
  if (WORLD_ANALYTIC) {
    if (!in_region(p)) { return terrain_sdf(p); }
    return world_d(p);
  }
  return cache_dg(p).x;
}

/** The disturbed channel at a hit (shading only). */
fn wg(p: vec3f) -> f32 {
  if (WORLD_ANALYTIC) {
    if (!in_region(p)) { return 0.0; }
    return world_all(p).y;
  }
  return cache_dg(p).y;
}

fn world_normal(p: vec3f) -> vec3f {
  // Bricks: half a voxel, so the four taps straddle the trilinear cells. Analytic: tight.
  let h = select(0.03, 0.004, WORLD_ANALYTIC);
  let k = vec2f(1.0, -1.0);
  return normalize(k.xyy * wd(p + k.xyy * h) + k.yyx * wd(p + k.yyx * h) +
                   k.yxy * wd(p + k.yxy * h) + k.xxx * wd(p + k.xxx * h));
}

/** Short-range ambient occlusion: three taps along the normal. */
fn world_ao(p: vec3f, n: vec3f) -> f32 {
  var occ = 0.0;
  occ += 0.5 * max(0.05 - wd(p + n * 0.05), 0.0) / 0.05;
  occ += 0.3 * max(0.15 - wd(p + n * 0.15), 0.0) / 0.15;
  occ += 0.2 * max(0.40 - wd(p + n * 0.40), 0.0) / 0.40;
  return clamp(1.0 - 1.2 * occ, 0.0, 1.0);
}

// ---- moving things ------------------------------------------------------------------------------------

fn march_creature(base: u32, mask: u32, o: vec3f, d: vec3f, t_start: f32, t_end: f32, c: ptr<function, Counters>) -> f32 {
  let np = countOneBits(mask);
  (*c).marches += 1u;
  var t = t_start;
  for (var i = 0u; i < MAX_STEPS; i++) {
    (*c).steps += 1u;
    (*c).parts += np;
    let eps = max(C_EPS * t * F.eye.w, 1e-4);
    let dist = cfield(base, mask, o + d * t);
    if (dist < eps) { return t + dist; }
    t += dist * C_STEP;
    if (t > t_end) { (*c).misses += 1u; return INF; }
  }
  (*c).caps += 1u;
  return INF;
}

fn shadow_march(base: u32, mask: u32, o: vec3f, d: vec3f, t0: f32, t1: f32, c: ptr<function, Counters>) -> f32 {
  (*c).sh_marches += 1u;
  var res = 1.0;
  var t = t0;
  var ph = 1e10;
  for (var i = 0u; i < SHADOW_STEPS; i++) {
    (*c).sh_steps += 1u;
    let h = cfield(base, mask, o + d * t);
    if (h < 1e-4) { return 0.0; }
    // The penumbra only counts within PEN of the surface, so binning (capsules inflated by PEN)
    // and the reference (bound spheres inflated by PEN) see the same shadow.
    if (h < PEN) {
      let y = h * h / (2.0 * ph);
      let dd = sqrt(max(h * h - y * y, 0.0));
      res = min(res, SOFT * dd / max(t - y, 1e-3));
      if (res < 0.005) { return 0.0; }
    }
    ph = h;
    t += max(h * C_STEP, 0.002);
    if (t > t1) { return res; }
  }
  (*c).sh_caps += 1u;
  return res;
}

/** Sun visibility through the moving things. Shadow rays are parallel, so a ray stays in one
 *  light-grid cell and that cell's list holds every candidate (spike 02). */
fn creature_shadow(o: vec3f, c: ptr<function, Counters>) -> f32 {
  let d = F.sun.xyz;
  var res = 1.0;
  if (REF) {
    for (var i = 0u; i < F.dims.w; i++) {
      let base = i * STRIDE;
      let s = inst[base + I_BOUND];
      let r = ray_sphere(o, d, s.xyz, s.w + PEN);
      if (r.y <= 0.0) { continue; }
      res = min(res, shadow_march(base, enabled_parts(base), o, d, max(r.x, 0.0), r.y, c));
      if (res <= 0.0) { break; }
    }
    return res;
  }
  let rel = o - F.lc.xyz;
  let g = f32(LGRID);
  let e = F.lc.w;
  let l = (vec2f(dot(rel, F.lr.xyz), dot(rel, F.lu.xyz)) + e) / (2.0 * e) * g;
  if (any(l < vec2f(0.0)) || any(l >= vec2f(g))) { return 1.0; }
  let cb = (u32(l.y) * LGRID + u32(l.x)) * LBIN_WORDS;
  let cnt = lbins[cb];
  for (var k = 0u; k < cnt; k++) {
    let base = lbins[cb + 1u + 2u * k] * STRIDE;
    let rm = ray_mask(base, lbins[cb + 2u + 2u * k], o, d, INF, PEN);
    if (rm.m == 0u) { continue; }
    res = min(res, shadow_march(base, rm.m, o, d, max(rm.t0, 0.0), rm.t1, c));
    if (res <= 0.0) { break; }
  }
  return res;
}

/** Sun visibility from a surface point. The offset is the same in every variant (not tied to a
 *  hit tolerance), so fast paths and the reference start their shadow rays from the same place. */
fn sun_vis(p: vec3f, n: vec3f, fp: f32, c: ptr<function, Counters>) -> f32 {
  if (!SHADOWS) { return 1.0; }
  let o = p + n * (0.02 + 0.5 * fp);
  var v = 1.0;
  if (WORLD_ANALYTIC) { v = shadow_analytic(o, c); } else { v = shadow_cache(o, c); }
  if (v > 0.0 && CREATURES && CREATURE_SHADOWS) { v = min(v, creature_shadow(o, c)); }
  return v;
}

// ---- shading ------------------------------------------------------------------------------------------

struct Surf { albedo: vec3f, n: vec3f, rough: f32, spec: f32 }

fn detail(fp: f32, size: f32) -> f32 { return 1.0 - smoothstep(0.15 * size, 0.6 * size, fp); }

fn tangent_u(n: vec3f) -> vec3f {
  let t = cross(vec3f(0.0, 1.0, 0.0), n);
  let l = length(t);
  return select(vec3f(1.0, 0.0, 0.0), t / max(l, 1e-6), l > 1e-3);
}

/** The direction down a sloped surface (zero on a level one). */
fn downslope(n: vec3f) -> vec3f {
  let v = vec3f(0.0, -1.0, 0.0) + n * n.y;
  let l = length(v);
  return select(vec3f(0.0), v / max(l, 1e-6), l > 1e-3);
}

fn rough_n(n: vec3f, p: vec3f, f: f32, amt: f32) -> vec3f {
  let q = p * f;
  return normalize(n + amt * (vec3f(noise3(q), noise3(q + 17.3), noise3(q + 31.7)) - 0.5));
}

fn masonry(uv: vec2f, n: vec3f, fp: f32, tint: vec3f) -> Surf {
  let course = 0.42;
  let row = floor(uv.y / course);
  let ri = i32(row);
  let blen = 0.72 + 0.25 * hash2(vec2i(ri, 7));
  let u = uv.x / blen + 0.5 * f32(ri & 1) + 0.3 * hash2(vec2i(ri, 3));
  let col = floor(u);
  let f = vec2f(fract(u) * blen, (uv.y / course - row) * course);
  let ex = min(f.x, blen - f.x);
  let ey = min(f.y, course - f.y);
  let edge = min(ex, ey);
  let w = detail(fp, 0.06);
  let h = hash2(vec2i(i32(col), ri));
  var alb = tint * (0.7 + 0.55 * h) * (0.82 + 0.36 * noise2(uv * 2.7 + h * 9.0));
  alb *= mix(0.62, 1.0, smoothstep(-0.5, 2.5, uv.y));           // damp at the foot
  let mortar = (1.0 - smoothstep(0.012, 0.03, edge)) * w;
  alb = mix(alb, tint * 0.5, mortar);
  let tu = tangent_u(n);
  let tv = normalize(cross(n, tu));
  var g = vec2f(0.0, select(-1.0, 1.0, f.y > 0.5 * course));
  if (ex < ey) { g = vec2f(select(-1.0, 1.0, f.x > 0.5 * blen), 0.0); }
  let bev = (1.0 - smoothstep(0.0, 0.06, edge)) * w * 0.7;
  var nn = normalize(n + (tu * g.x + tv * g.y) * bev);
  nn = rough_n(nn, vec3f(uv, h * 5.0), 9.0, 0.25 * w);
  return Surf(alb, nn, 0.85, 0.025);
}

fn cobbles(p: vec3f, n: vec3f, fp: f32) -> Surf {
  let rw = 0.23;
  let row = floor(p.z / rw);
  let ri = i32(row);
  let u = p.x / 0.29 + hash2(vec2i(ri, 3));
  let col = floor(u);
  let f = vec2f(u - col - 0.5, p.z / rw - row - 0.5);
  let h = hash2(vec2i(i32(col), ri));
  let w = detail(fp, 0.05);
  let e = max(abs(f.x), abs(f.y)) * 2.0;
  let stone = 1.0 - smoothstep(0.72, 0.95, e + 0.1 * noise2(p.xz * 13.0));
  let sc = mix(vec3f(0.15, 0.14, 0.125), vec3f(0.29, 0.25, 0.20), h) * (0.85 + 0.3 * noise2(p.xz * 5.0));
  let gap = vec3f(0.06, 0.05, 0.038);
  var alb = mix(vec3f(0.165, 0.148, 0.124), mix(gap, sc, stone), w);
  alb *= 0.62 + 0.55 * fbm2(p.xz * 0.21);                        // dirt and wear
  alb = mix(alb, vec3f(0.13, 0.105, 0.075), 0.5 * smoothstep(0.55, 0.75, fbm2(p.xz * 0.07 + 3.0)));
  let nn = normalize(n + vec3f(f.x, 0.0, f.y) * 1.1 * stone * w);
  return Surf(alb, nn, 0.65, 0.04);
}

fn grass(p: vec3f, n: vec3f, fp: f32) -> Surf {
  let big = fbm2(p.xz * 0.06);
  let mid = noise2(p.xz * 0.9);
  var alb = mix(vec3f(0.04, 0.075, 0.02), vec3f(0.115, 0.13, 0.038), big);
  alb *= 0.78 + 0.4 * mid * detail(fp, 0.4);
  alb = mix(alb, vec3f(0.15, 0.13, 0.07), 0.55 * smoothstep(0.6, 0.78, fbm2(p.xz * 0.045 + 7.0)));
  // A dirt road leaving the square to the south, and wear around its edge.
  let road = (1.0 - smoothstep(1.3, 2.4, abs(p.x - 3.0 - 4.0 * sin(p.z * 0.045)))) * smoothstep(-13.0, -16.0, p.z);
  let edge = 1.0 - smoothstep(0.0, 3.5, rect_sd(p.x, p.z, PLAZA));
  alb = mix(alb, vec3f(0.16, 0.125, 0.08) * (0.8 + 0.3 * mid), max(road, 0.6 * edge));
  let nn = rough_n(n, p, 2.1, 0.45 * detail(fp, 0.3));
  return Surf(alb, nn, 0.95, 0.015);
}

fn tile_roof(uv: vec2f, n: vec3f, fp: f32, seed: f32) -> Surf {
  let rowh = 0.28;
  let row = floor(uv.y / rowh);
  let ri = i32(row);
  let u = uv.x / 0.24 + 0.5 * f32(ri & 1);
  let col = floor(u);
  let fy = uv.y / rowh - row;
  let fx = u - col - 0.5;
  let h = hash2(vec2i(i32(col), ri + i32(seed * 1000.0)));
  let w = detail(fp, 0.06);
  var alb = mix(vec3f(0.28, 0.085, 0.045), vec3f(0.35, 0.16, 0.075), seed) * (0.75 + 0.45 * h);
  alb *= 1.0 - 0.45 * smoothstep(0.7, 1.0, fy) * w;
  alb *= 1.0 - 0.3 * smoothstep(0.38, 0.5, abs(fx)) * w;
  alb = mix(alb, vec3f(0.08, 0.085, 0.045), 0.5 * smoothstep(0.55, 0.8, fbm2(uv * 0.7 + seed * 10.0)));
  let down = downslope(n);
  let tu = tangent_u(n);
  let nn = normalize(n + down * (fy - 0.5) * 0.7 * w + tu * fx * 0.8 * w);
  return Surf(alb, nn, 0.55, 0.04);
}

fn slate(uv: vec2f, n: vec3f, fp: f32) -> Surf {
  let rowh = 0.2;
  let row = floor(uv.y / rowh);
  let ri = i32(row);
  let u = uv.x / 0.26 + 0.5 * f32(ri & 1);
  let col = floor(u);
  let fy = 1.0 - (uv.y / rowh - row);
  let h = hash2(vec2i(i32(col), ri));
  let w = detail(fp, 0.05);
  var alb = vec3f(0.055, 0.062, 0.078) * (0.75 + 0.5 * h);
  alb *= 1.0 - 0.4 * smoothstep(0.75, 1.0, fy) * w;
  let down = downslope(n);
  let nn = normalize(n + down * (fy - 0.5) * 0.6 * w);
  return Surf(alb, nn, 0.4, 0.05);
}

fn plaster(uv: vec2f, n: vec3f, fp: f32, seed: f32) -> Surf {
  let w = detail(fp, 0.12);
  var alb = mix(vec3f(0.60, 0.55, 0.43), vec3f(0.64, 0.47, 0.32), fract(seed * 7.3));
  alb *= 0.86 + 0.26 * fbm2(uv * 1.1 + seed * 20.0);
  alb *= mix(0.72, 1.0, smoothstep(0.0, 1.8, uv.y));
  let timber = vec3f(0.07, 0.042, 0.026);
  let pu = uv.x / 1.6;
  let pi = floor(pu);
  let pf = pu - pi;
  let post = (1.0 - smoothstep(0.075, 0.095, min(pf, 1.0 - pf) * 1.6));
  let rail = max(1.0 - smoothstep(0.08, 0.1, abs(uv.y - 0.35)), 1.0 - smoothstep(0.08, 0.1, abs(uv.y - 2.85)));
  var beam = max(post, rail);
  if (uv.y > 2.85 && (i32(pi) & 1) == 0) {
    beam = max(beam, 1.0 - smoothstep(0.06, 0.085, abs(pf * 1.6 - (uv.y - 2.85) * 0.68) * 0.83));
  }
  let wx = abs(pf - 0.5) * 1.6;
  let upper = (i32(pi) & 1) == 1 && uv.y > 3.3 && uv.y < 4.4 && wx < 0.36;
  let lower = (((i32(pi) % 3) + 3) % 3) == 0 && uv.y > 0.9 && uv.y < 2.1 && wx < 0.38;
  alb = mix(alb, timber, beam * w);
  var spec = 0.02;
  var rough = 0.9;
  if ((upper || lower) && w > 0.0) {
    let frame = select(0.0, 1.0, wx > 0.3 || fract((uv.y - 0.9) / 0.6) < 0.06);
    alb = mix(alb, mix(vec3f(0.015, 0.02, 0.026), timber, frame), w);
    spec = mix(0.02, 0.3, (1.0 - frame) * w);
    rough = mix(0.9, 0.12, (1.0 - frame) * w);
  }
  return Surf(alb, rough_n(n, vec3f(uv, seed), 3.0, 0.12 * w), rough, spec);
}

fn world_surface(p: vec3f, n: vec3f, fp: f32, g: f32) -> Surf {
  let m = base_material(p);
  var s: Surf;
  switch (m.id) {
    case M_COBBLE: { s = cobbles(p, n, fp); }
    case M_TOWER: { s = masonry(m.uv, n, fp, vec3f(0.33, 0.30, 0.26)); }
    case M_WALL: { s = masonry(m.uv, n, fp, vec3f(0.30, 0.285, 0.255)); }
    case M_SLATE: { s = slate(m.uv, n, fp); }
    case M_PLASTER: { s = plaster(m.uv, n, fp, m.seed); }
    case M_TILE: { s = tile_roof(m.uv, n, fp, m.seed); }
    case M_DOOR: {
      let plank = 1.0 - smoothstep(0.0, 0.02, abs(fract(p.x * 6.0) - 0.5) - 0.46);
      s = Surf(vec3f(0.06, 0.035, 0.02) * (1.0 - 0.4 * plank), n, 0.7, 0.03);
    }
    default: { s = grass(p, n, fp); }
  }
  // Edits: freshly turned earth (and scorch at a crater's heart), or broken stone.
  if (g > 0.02) {
    let earth = vec3f(0.13, 0.095, 0.062) * (0.6 + 0.6 * noise3(p * 3.1)) * mix(1.0, 0.45, smoothstep(0.9, 1.0, g));
    let k = smoothstep(0.0, 0.6, g);
    s.albedo = mix(s.albedo, earth, k);
    s.n = normalize(mix(s.n, rough_n(n, p, 5.0, 0.6), k));
    s.rough = mix(s.rough, 0.98, k);
  } else if (g < -0.02) {
    let fresh = vec3f(0.25, 0.235, 0.21) * (0.7 + 0.45 * noise3(p * 6.3));
    let k = smoothstep(0.0, 0.6, -g);
    s.albedo = mix(s.albedo, fresh, k);
    s.n = normalize(mix(s.n, rough_n(n, p, 7.0, 0.7), k));
  }
  return s;
}

const SUN_RGB = vec3f(3.1, 2.8, 2.3);

fn light(s: Surf, p: vec3f, sh: f32, ao: f32) -> vec3f {
  let l = F.sun.xyz;
  let v = normalize(F.eye.xyz - p);
  let h = normalize(l + v);
  let ndl = max(dot(s.n, l), 0.0);
  let a = max(s.rough * s.rough, 0.02);
  let sp = 2.0 / (a * a) - 2.0;
  let spec = pow(max(dot(s.n, h), 0.0), sp) * (sp + 8.0) / 25.13 * s.spec;
  let skyc = mix(vec3f(0.20, 0.17, 0.13), vec3f(0.36, 0.46, 0.62), 0.5 + 0.5 * s.n.y);
  return s.albedo * (SUN_RGB * ndl * sh + skyc * ao) + SUN_RGB * spec * ndl * sh;
}

fn sky(d: vec3f) -> vec3f {
  let y = max(d.y, 0.0);
  var col = mix(vec3f(0.60, 0.66, 0.74), vec3f(0.20, 0.36, 0.66), pow(y, 0.55));
  let sd = max(dot(d, F.sun.xyz), 0.0);
  col += vec3f(1.0, 0.82, 0.58) * (0.22 * pow(sd, 6.0) + 0.5 * pow(sd, 64.0));
  if (d.y > 0.01) {
    let uv = d.xz / d.y * 1.3 + vec2f(F.sun.w * 0.004, 0.0);
    let n = fbm2(uv * 1.4);
    let cl = smoothstep(0.45, 0.78, n) * smoothstep(0.02, 0.2, d.y);
    let lit = mix(vec3f(0.62, 0.64, 0.68), vec3f(1.05, 1.0, 0.94), smoothstep(0.45, 0.8, n + 0.15 * sd));
    col = mix(col, lit, cl * 0.9);
  }
  return col;
}

fn haze(d: vec3f) -> vec3f {
  let sd = max(dot(d, F.sun.xyz), 0.0);
  return vec3f(0.56, 0.62, 0.70) + vec3f(0.30, 0.22, 0.12) * pow(sd, 4.0);
}

fn tonemap(x: vec3f) -> vec3f {
  let c = x * 0.75;
  return clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), vec3f(0.0), vec3f(1.0));
}

fn heat(x: f32) -> vec3f {
  let t = clamp(x, 0.0, 1.0);
  return clamp(vec3f(1.5 * t, 1.5 * t * t, 0.5 - t) + vec3f(0.0, 0.0, 0.2) * (1.0 - t), vec3f(0.0), vec3f(1.0));
}

// ---- the kernel -------------------------------------------------------------------------------------

@compute @workgroup_size(8, 8)
fn trace(@builtin(global_invocation_id) gid: vec3u) {
  let px = vec2u(gid.x + F.misc.z, gid.y + F.misc.x);   // tile offset (heavy variants render in tiles)
  if (px.x >= F.dims.x || px.y >= F.dims.y) { return; }
  var c = Counters(0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u);
  let u = (f32(px.x) + 0.5) / f32(F.dims.x) * 2.0 - 1.0;
  let v = 1.0 - (f32(px.y) + 0.5) / f32(F.dims.y) * 2.0;
  let o = F.eye.xyz;
  let d = normalize(F.cf.xyz + F.cr.xyz * u + F.cu.xyz * v);

  // The static world: terrain before the region, the region (bricks or analytic), terrain beyond.
  var tw = INF;
  var far = true;
  let span = ray_box(o, d, RMIN, RMAX);
  let inside = span.y > max(span.x, 0.0);
  if (inside) {
    if (span.x > 0.0) { tw = terrain_trace(o, d, 0.0, span.x, &c); }
    if (tw >= INF) {
      if (WORLD_ANALYTIC) { tw = march_analytic(o, d, max(span.x, 0.0), span.y, &c); }
      else { tw = march_cache(o, d, max(span.x, 0.0), span.y, &c); }
      far = tw >= INF;
    }
    if (tw >= INF) { tw = terrain_trace(o, d, span.y, INF, &c); }
  } else {
    tw = terrain_trace(o, d, 0.0, INF, &c);
  }

  // Moving things: the tile's list (or, for the reference, everyone), bounded by the world hit.
  var best = tw;
  var hit_base = 0xFFFFFFFFu;
  var hit_mask = 0u;
  if (CREATURES) {
    if (REF) {
      for (var i = 0u; i < F.dims.w; i++) {
        let base = i * STRIDE;
        let s = inst[base + I_BOUND];
        let rs = ray_sphere(o, d, s.xyz, s.w);
        if (rs.y <= 0.0 || rs.x >= best) { continue; }
        let m = enabled_parts(base);
        let t = march_creature(base, m, o, d, max(rs.x, 0.0), min(rs.y, best), &c);
        if (t < best) { best = t; hit_base = base; hit_mask = m; }
      }
    } else {
      let tb = ((px.y / TILE) * F.dims.z + px.x / TILE) * BIN_WORDS;
      let n = tiles[tb];
      c.entries = n;
      for (var e = 0u; e < n; e++) {
        let base = tiles[tb + 1u + 2u * e] * STRIDE;
        let s = inst[base + I_BOUND];
        c.spheres += 1u;
        let rs = ray_sphere(o, d, s.xyz, s.w);
        if (rs.y <= 0.0 || rs.x >= best) { continue; }
        let rm = ray_mask(base, tiles[tb + 2u + 2u * e], o, d, best, 0.0);
        if (rm.m == 0u) { continue; }
        let t = march_creature(base, rm.m, o, d, max(rm.t0, 0.0), min(rm.t1, best), &c);
        if (t < best) { best = t; hit_base = base; hit_mask = rm.m; }
      }
    }
  }

  // Shade whichever is nearest. One sun-visibility call site, so it's inlined once.
  var color = vec3f(0.0);
  let fp = best * F.eye.w;
  let p = o + d * best;
  var s = Surf(vec3f(0.0), vec3f(0.0, 1.0, 0.0), 0.9, 0.02);
  var ng = vec3f(0.0, 1.0, 0.0);
  var ao = 1.0;
  if (hit_base != 0xFFFFFFFFu) {
    let cs = cfield_g(hit_base, hit_mask, p);
    ng = normalize(cs.g);
    s = Surf(mix(inst[hit_base + I_COLA].xyz, inst[hit_base + I_COLB].xyz, cs.ch), ng, 0.75, 0.03);
    ao = 0.55 + 0.45 * clamp(0.5 + 0.5 * ng.y, 0.0, 1.0);
  } else if (best < INF) {
    if (!far) {
      ng = world_normal(p);
      if (SHADE) {
        s = world_surface(p, ng, fp, wg(p));
        ao = world_ao(p, ng);
      } else {
        s = Surf(vec3f(0.2), ng, 0.9, 0.02);
      }
    } else {
      let e = 0.05 * max(1.0, best * 0.02);
      ng = normalize(vec3f(terrain_h(p.x - e, p.z) - terrain_h(p.x + e, p.z), 2.0 * e,
                           terrain_h(p.x, p.z - e) - terrain_h(p.x, p.z + e)));
      s = grass(p, ng, fp);
    }
  }
  if (best < INF) {
    var sh = 0.0;
    if (dot(ng, F.sun.xyz) > 0.0) { sh = sun_vis(p, ng, fp, &c); }
    color = light(s, p, sh, ao);
  } else {
    color = sky(d);
  }
  if (best < INF) {
    color = mix(color, haze(d), 1.0 - exp(-best / 650.0));
  }
  color = tonemap(color);

  if (VIEW == 1u) { color = heat(f32(c.w_steps + c.w_empty + c.t_steps) / 96.0); }
  if (VIEW == 2u && c.marches > 0u) { color = heat(f32(c.steps) / 48.0); }
  if (VIEW == 3u && (c.ws_marches + c.sh_marches) > 0u) { color = heat(f32(c.ws_steps + c.sh_steps) / 64.0); }
  textureStore(out_tex, vec2i(px), vec4f(color, 1.0));

  if (STATS) {
    if (hit_base != 0xFFFFFFFFu) {
      atomicAdd(&stats[S_CREATURE_PX], 1u);
      atomicAdd(&stats[S_HIT_STEPS], c.steps);
    } else if (best < INF) {
      atomicAdd(&stats[S_WORLD_PX], 1u);
      if (far) { atomicAdd(&stats[S_FAR_PX], 1u); }
    } else {
      atomicAdd(&stats[S_SKY_PX], 1u);
    }
    atomicAdd(&stats[S_MARCHES], c.marches);
    atomicAdd(&stats[S_STEPS], c.steps);
    atomicAdd(&stats[S_MISSES], c.misses);
    atomicAdd(&stats[S_CAPS], c.caps);
    if (c.sh_marches + c.ws_marches > 0u) { atomicAdd(&stats[S_SHADOW_PX], 1u); }
    atomicAdd(&stats[S_SH_MARCHES], c.sh_marches);
    atomicAdd(&stats[S_SH_STEPS], c.sh_steps);
    atomicAdd(&stats[S_SH_CAPS], c.sh_caps);
    atomicAdd(&stats[S_W_STEPS], c.w_steps);
    atomicAdd(&stats[S_W_EMPTY], c.w_empty);
    atomicAdd(&stats[S_W_CAPS], c.w_caps);
    atomicAdd(&stats[S_WS_STEPS], c.ws_steps);
    atomicAdd(&stats[S_WS_CAPS], c.ws_caps);
    atomicAdd(&stats[S_WS_MARCHES], c.ws_marches);
    atomicAdd(&stats[S_T_STEPS], c.t_steps);
    atomicAdd(&stats[S_ENTRIES], c.entries);
    atomicAdd(&stats[S_PART_EVALS], c.parts);
    atomicAdd(&stats[S_SPHERE_TESTS], c.spheres);
  }
}
