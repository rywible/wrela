// Primary visibility: one compute kernel, specialized per variant by pipeline-overridable constants.
// Appended to common.wgsl + field.wgsl + cache.wgsl. Writes the G-buffer (normal, material) and t.
//
//   MODE 0 analytic  sphere-trace the field (directional bounds), no cache
//   MODE 1 cache     march the cache; normal from the cache (FIELD_NORMALS: from the field)
//   MODE 2 hybrid    march the cache as a conservative bound (cached − delta), switch to the
//                    field near the surface. Exact under the field's Lipschitz facts.

@group(0) @binding(5) var<storage, read_write> stats: array<atomic<u32>, 64>;
@group(1) @binding(0) var gbuf: texture_storage_2d<rgba16float, write>;
@group(1) @binding(1) var gdepth: texture_storage_2d<r32float, write>;

override MODE: u32 = 1u;
override FIELD_NORMALS: bool = false;
override STATS: bool = false;
override STEP_SCALE: f32 = 1.0;        // < 1 for the reference
override EPS_SCALE: f32 = 0.25;        // hit tolerance, in pixel footprints
override MAX_STEPS: u32 = 512u;          // the hybrid uses 2000 (see main.js)
override RELAX: f32 = 1.6;             // over-relaxation in the cache march (Keinert et al. 2014)

const T_NEAR: f32 = 0.05;

// Stats slots.
const S_HIT: u32 = 0u;
const S_SKY: u32 = 1u;
const S_STEPS: u32 = 2u;
const S_FIELD: u32 = 3u;      // field evaluations in the march (analytic or hybrid's near phase)
const S_EMPTY: u32 = 4u;      // steps through empty bricks
const S_CAP: u32 = 5u;
const S_OUT: u32 = 6u;        // rays that left the cache before the view distance
const S_STONE: u32 = 7u;
const S_LEVEL0: u32 = 16u;    // 16..29: hits per level

struct Hit { t: f32, ok: bool, steps: u32, field: u32, empty: u32, cap: bool, out: bool, level: u32 }

fn content(rel: vec3f) -> vec3f {
  // Shader-world position, rounded the way an absolute-coordinate engine would, then moved into
  // content space. With shift = 0 this is just cam + rel.
  let ps = F.cam.xyz + rel;
  return ps - F.shift.xyz;
}

/** The ray's extent: the y-slab that holds all geometry, and the view distance. */
fn ray_end(dir: vec3f) -> f32 {
  let y = F.cam.y - F.shift.y;
  var t1 = F.shift.w;
  if (dir.y > 1e-6) { t1 = min(t1, (T_YMAX - y) / dir.y); }
  if (dir.y < -1e-6) { t1 = min(t1, (T_YMIN - y) / dir.y); }
  return t1;
}

/** A ray that ran out of steps: a grazing ray still close to the surface counts as a hit there
 *  (it converges slowly, not wrongly); anything else is a miss. Both are counted as caps. */
fn capped(h0: Hit, t: f32, last: f32) -> Hit {
  var h = h0;
  h.cap = true;
  if (last < 8.0) { h.t = t; h.ok = true; }
  return h;
}

fn march_analytic(dir: vec3f, t1: f32) -> Hit {
  var h = Hit(INF, false, 0u, 0u, 0u, false, false, 0u);
  var last = INF;
  var t = T_NEAR;
  var t_prev = t;
  var g_prev = INF;
  for (var i = 0u; i < MAX_STEPS; i++) {
    h.steps += 1u;
    let fp = t * F.cam.w;
    let eps = max(EPS_SCALE * fp, 1e-4);
    let s = world_sample(content(dir * t), dir, fp);
    h.field += 1u;
    last = s.d / eps;
    if (s.d < eps) {
      // Terrain: a secant through the last two gaps lands near the surface. Otherwise one more step.
      if (abs(s.d - s.gap * T_K) < 1e-6 && g_prev < INF && g_prev > s.gap) {
        t = t - s.gap * (t - t_prev) / (g_prev - s.gap);
      } else {
        t += s.d;
      }
      h.t = t;
      h.ok = true;
      return h;
    }
    t_prev = t;
    g_prev = s.gap;
    t += s.s * STEP_SCALE;
    if (t > t1) { return h; }
  }
  return capped(h, t, last);
}

fn march_cache(dir: vec3f, t1: f32) -> Hit {
  var h = Hit(INF, false, 0u, 0u, 0u, false, false, 0u);
  var last = INF;
  var t = T_NEAR;
  var L = 0u;
  var w = RELAX;
  var t_prev = t;
  var r_prev = 0.0;
  var relaxed = false;     // the previous step was an over-relaxed sphere step
  for (var i = 0u; i < MAX_STEPS; i++) {
    h.steps += 1u;
    let c = cache_from(dir * t, dir, L);
    if (c.kind == 2u) { h.out = true; return h; }
    L = c.level;
    h.level = L;
    let eps = max(EPS_SCALE * t * F.cam.w, 1e-4);
    last = c.d / eps;
    if (c.kind == 1u) {
      h.empty += 1u;
      if (c.d < 0.0) { h.t = t; h.ok = true; return h; }
      t += max(c.d, c.exit + 1e-3 * F.lv[L].rel.w);
      relaxed = false;
    } else {
      if (relaxed && c.d + r_prev < t - t_prev) {
        // The relaxed step left a gap between the two spheres: go back to the safe step.
        t = t_prev + r_prev;
        relaxed = false;
        w = 1.0;
        continue;
      }
      if (c.d < eps) { h.t = t + c.d; h.ok = true; return h; }
      t_prev = t;
      r_prev = c.d;
      t += c.d * w;
      relaxed = w > 1.0;
    }
    if (t > t1) { return h; }
  }
  return capped(h, t, last);
}

fn march_hybrid(dir: vec3f, t1: f32) -> Hit {
  var h = Hit(INF, false, 0u, 0u, 0u, false, false, 0u);
  var last = INF;
  var Lh = 0u;
  var t = T_NEAR;
  var t_prev = t;
  var g_prev = INF;
  for (var i = 0u; i < MAX_STEPS; i++) {
    h.steps += 1u;
    let rel = dir * t;
    let c = cache_from(rel, dir, Lh);
    if (c.kind == 2u) { h.out = true; return h; }
    Lh = c.level;
    let fp = t * F.cam.w;
    let eps = max(EPS_SCALE * fp, 1e-4);
    var step: f32;
    if (c.kind == 1u) {
      // No surface in this brick (under the field's facts): skip to its exit.
      h.empty += 1u;
      if (c.d < 0.0) { h.t = t; h.ok = true; h.level = c.level; return h; }
      step = max(c.d, c.exit + 1e-3 * F.lv[c.level].rel.w);
      g_prev = INF;
    } else {
      let cb = c.d - F.lv[c.level].bnd.y;   // a lower bound on the true field
      last = cb / eps;
      h.level = c.level;
      if (cb > eps) {
        step = cb;
        g_prev = INF;
      } else {
        let s = world_sample(content(rel), dir, fp);
        h.field += 1u;
        last = s.d / eps;
        if (s.d < eps) {
          if (abs(s.d - s.gap * T_K) < 1e-6 && g_prev < INF && g_prev > s.gap) {
            t = t - s.gap * (t - t_prev) / (g_prev - s.gap);
          } else {
            t += s.d;
          }
          h.t = t;
          h.ok = true;
          h.level = c.level;
          return h;
        }
        g_prev = s.gap;
        step = max(s.s * STEP_SCALE, cb);
      }
    }
    t_prev = t;
    t += step;
    if (t > t1) { return h; }
  }
  return capped(h, t, last);
}

fn heat_level(L: u32, n: u32) -> f32 {
  return f32(L) + f32(min(n, 999u)) / 1000.0;
}

@compute @workgroup_size(8, 8)
fn march(@builtin(global_invocation_id) gid3: vec3u) {
  let gid = vec2u(gid3.x, gid3.y + F.dbg.z);     // dbg.z: row offset, for the banded reference
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let dir = ray_dir(vec2f(gid.xy) + 0.5);
  let t1 = ray_end(dir);
  var h: Hit;
  if (MODE == 0u) { h = march_analytic(dir, t1); }
  else if (MODE == 1u) { h = march_cache(dir, t1); }
  else { h = march_hybrid(dir, t1); }

  var g = vec4f(0.0);
  var tt = INF;
  if (h.ok) {
    tt = h.t;
    let rel = dir * h.t;
    let fp = h.t * F.cam.w;
    let pc = content(rel);
    var n: vec3f;
    var stone = 0.0;
    if (MODE == 1u && !FIELD_NORMALS) {
      let cell = F.lv[h.level].bnd.x;
      n = cache_normal(rel, 0.5 * cell);
      if ((PARTS & P_RUINS) != 0u && ruins_d(pc) < 0.75 * cell) { stone = 1.0; }
    } else {
      n = world_normal(pc, fp);
      if ((PARTS & P_RUINS) != 0u) {
        let s = world_sample(pc, vec3f(0.0), fp);
        if (s.ruin <= s.d + 1e-4) { stone = 1.0; }
      }
    }
    if (MODE == 0u) { h.level = min(level_guess(rel), F.dims.z - 1u); }
    var heat = h.steps;
    if (F.dbg.x == 3u) { heat = h.field; }
    g = vec4f(oct_encode(n), stone, heat_level(h.level, heat));
    if (STATS) {
      atomicAdd(&stats[S_HIT], 1u);
      atomicAdd(&stats[S_LEVEL0 + min(h.level, 13u)], 1u);
      if (stone > 0.0) { atomicAdd(&stats[S_STONE], 1u); }
    }
  } else {
    g = vec4f(0.0, 0.0, 0.0, heat_level(0u, select(h.steps, h.field, F.dbg.x == 3u)));
    if (STATS) { atomicAdd(&stats[S_SKY], 1u); }
  }
  textureStore(gbuf, vec2i(gid.xy), g);
  textureStore(gdepth, vec2i(gid.xy), vec4f(tt, 0.0, 0.0, 0.0));
  if (STATS) {
    atomicAdd(&stats[S_STEPS], h.steps);
    atomicAdd(&stats[S_FIELD], h.field);
    atomicAdd(&stats[S_EMPTY], h.empty);
    if (h.cap) { atomicAdd(&stats[S_CAP], 1u); }
    if (h.out) { atomicAdd(&stats[S_OUT], 1u); }
  }
}
