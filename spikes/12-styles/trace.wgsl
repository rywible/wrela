// Primary visibility: one sphere-tracing march per pixel, writing the G-buffer and the
// closest-approach record that field-native lines and stroke overshoot use.
// Appended to common.wgsl + placement + wolf.wgsl + scene.wgsl.

@group(0) @binding(2) var gb_out: texture_storage_2d<rgba32uint, write>;
@group(0) @binding(3) var<storage, read_write> stats: array<atomic<u32>, 32>;

override LINES: bool = true;        // track the closest-approach record
override HALO_REACH: bool = false;  // lines feed the painterly overshoot, which reaches further than an outline
override STATS: bool = false;
override STEP_SCALE: f32 = 1.0;     // < 1 for the reference
override EPS_SCALE: f32 = 0.1;      // hit tolerance in pixel footprints (0.25 fails the reference check)
override MAX_STEPS: u32 = 400u;

const TMAX: f32 = 700.0;
const T_NEAR: f32 = 0.05;

// Stats slots.
const S_PX: u32 = 0u;               // 0–6: pixels per material
const S_STEPS: u32 = 8u;
const S_CAPS: u32 = 9u;
const S_LINE_PX: u32 = 10u;
const S_COVERED: u32 = 11u;
const S_LEAF_STEPS: u32 = 12u;
const S_WOLF_STEPS: u32 = 13u;

@compute @workgroup_size(8, 8)
fn trace(@builtin(global_invocation_id) gid0: vec3u) {
  let gid = vec3u(gid0.xy + F.dims.zw, 0u);         // + the tile origin (0 for a whole frame)
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let o = F.eye.xyz;
  let d = ray_dir(gid.xy);
  let pa = F.eye.w;
  let leave = 2.0 * F.prm.x + 2.0;            // px: an approach counts once the ray gets this far away
  // Within this many px of a surface, distances must be exact: a bound (a bounding sphere, or the
  // displaced shell's base minus amplitude) would draw lines around the bound, not the surface.
  let reach = select(0.0, select(F.prm.x + 1.0, max(F.prm.x + 1.0, F.prm2.x), HALO_REACH), LINES);

  var t = T_NEAR;
  var mat = MAT_SKY;
  var steps = 0u;
  var capped = false;
  var t_prev = t;
  var gap_prev = INF;
  // closest-approach record: the current approach, and the committed (best) one
  var cur = INF;           // closest approach of the current approach, px (with between-sample estimates)
  var cur_s = INF;         // the same from samples only: decides when the ray has left
  var cur_t = 0.0;
  var cur_m = 0u;
  var line = INF;
  var line_t = 0.0;
  var line_m = 0u;
  var prev = INF;

  loop {
    if (steps >= MAX_STEPS) { capped = true; break; }
    steps++;
    let p = o + d * t;
    let fp = t * pa;
    let eps = max(EPS_SCALE * fp, 1e-4);
    let gap = p.y - terrain_h(p.x, p.z);
    if (gap < eps) {
      // secant through the last two samples, so the hit lands on the terrain
      if (gap_prev < INF && gap_prev > gap) { t = t_prev + (t - t_prev) * gap_prev / (gap_prev - gap); }
      mat = MAT_GROUND;
      break;
    }
    // the terrain's safe step, from the slope bound of the zone the ray is in
    let z = slope_zone(p, d);
    let rate = -d.y + z.s * length(d.xz);
    var sg = z.exit;
    if (rate > 0.0) { sg = min(sg, gap / rate * STEP_SCALE); }
    if (gap > TOP_ALL) {
      // above everything but the terrain: nothing to evaluate
      if (rate <= 0.0 && z.exit >= INF) { break; }
      if (LINES && cur < line) { line = cur; line_t = cur_t; line_m = cur_m; }
      cur = INF;
      cur_s = INF;
      prev = INF;
      t_prev = t;
      gap_prev = gap;
      t += min(sg, select(INF, (gap - TOP_ALL) / rate + 0.01, rate > 0.0));
      if (t > TMAX) { break; }
      continue;
    }
    let trees_on = gap < TREE_TOP;
    let h = scene(p, fp, max(max(4.0 * eps, 0.02), reach * fp), trees_on);
    if (h.d < eps) { t += h.d; mat = h.mat; break; }

    if (LINES) {
      let dd = min(h.d, gap * T_COS);
      let dpx = dd / fp;
      // closest approach between this sample and the last
      let ca = closest_approach(prev, dd, t - t_prev);
      let ta = max(t - ca.y, T_NEAR);
      let apx = min(dpx, ca.x / (ta * pa));
      if (apx < cur) { cur = apx; cur_t = ta; cur_m = select(MAT_GROUND, h.mat, h.d < gap * T_COS); }
      cur_s = min(cur_s, dpx);
      // commit only once the ray has clearly moved away again: a ray closing on its hit never does
      if (dpx > max(leave, 2.0 * cur_s + 2.0)) {
        if (cur < line) { line = cur; line_t = cur_t; line_m = cur_m; }
        cur = INF;
        cur_s = INF;
      }
      prev = dd;
    }

    var s = min(h.s * STEP_SCALE, sg);
    if (trees_on) { s = min(s, tree_limit(p, d, tree_index(p))); }
    else if (rate > 0.0) { s = min(s, (gap - TREE_TOP) / rate + 0.01); }   // don't step into the tree layer
    t_prev = t;
    gap_prev = gap;
    t += s;
    if (t > TMAX) { break; }
  }
  if (LINES && mat == MAT_SKY && !capped && cur < line) { line = cur; line_t = cur_t; line_m = cur_m; }

  var g: GB;
  g.t = select(t, INF, mat == MAT_SKY && !capped);
  g.mat = mat;
  g.steps = steps;
  g.line_px = line;
  g.line_t = line_t;
  g.line_mat = line_m;
  g.n = vec3f(0.0, 1.0, 0.0);
  if (mat == MAT_GROUND) {
    let p = o + d * t;
    g.n = terrain_n(p.x, p.z, max(0.02, t * pa));
  } else if (mat != MAT_SKY) {
    let p = o + d * t;
    g.n = scene_n(p, t * pa, max(0.5 * t * pa, 2e-4));
  }
  textureStore(gb_out, vec2i(gid.xy), gb_pack(g));

  if (STATS) {
    atomicAdd(&stats[S_PX + mat], 1u);
    atomicAdd(&stats[S_STEPS], steps);
    if (capped) { atomicAdd(&stats[S_CAPS], 1u); }
    if (line < F.prm.x) { atomicAdd(&stats[S_LINE_PX], 1u); }
    if (mat != MAT_SKY) { atomicAdd(&stats[S_COVERED], 1u); }
    if (mat == MAT_LEAF) { atomicAdd(&stats[S_LEAF_STEPS], steps); }
    if (mat == MAT_WOLF) { atomicAdd(&stats[S_WOLF_STEPS], steps); }
  }
}
