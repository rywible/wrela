// The path-traced reference. Appended after light.wgsl. Same primary hits (it reads the G-buffer),
// same light transport as the real-time path, estimated by brute force on the analytic field:
//   sun:    next-event estimation over the sun disc, hard shadow rays
//   sky:    one cosine-sampled ray; sky radiance if it escapes
//   bounce: if that ray hits, the hit's direct sun (one more disc sample) and sky (one more cosine
//           ray), times its albedo. Exactly one diffuse bounce, as in the real-time path.
//
// Two estimators, two dispatch modes (REF_MODE):
//   0 sun:      every pixel, stratified disc samples. Cheap (one shadow ray a sample) and low-noise.
//   1 indirect: sky + bounce on a uniform grid, one pixel in STRIDE² (at offset STRIDE/4 in each
//               axis), so each traced pixel gets STRIDE² times the samples for the same cost. The
//               metrics use exactly these pixels; elsewhere the indirect light is reconstructed for
//               display only (ref_resolve).
// Samples alternate between accumulators A (acc0, acc1) and B (acc2, acc3): two independent
// estimates, so comparing them measures the reference's own noise.
//
// Marching (march_ref in scenes.wgsl) uses half-size steps, a twentieth of the real-time hit
// tolerance and a doubled terrain slope bound. In empty space it may skip with the cooked clipmap's
// distance minus a provable margin (see ref_objects): that changes the cost, not the answer.

override REF_MODE: u32 = 0u;

@group(1) @binding(34) var<storage, read_write> acc0: array<vec4f>;   // A: sky rgb, sun Σ n·l·V
@group(1) @binding(35) var<storage, read_write> acc1: array<vec4f>;   // A: bounce rgb, indirect samples
@group(1) @binding(36) var<storage, read_write> acc2: array<vec4f>;   // B
@group(1) @binding(37) var<storage, read_write> acc3: array<vec4f>;

fn rnd(s: ptr<function, u32>) -> f32 {
  *s = *s * 747796405u + 2891336453u;
  let w = ((*s >> ((*s >> 28u) + 4u)) ^ *s) * 277803737u;
  return f32(((w >> 22u) ^ w) >> 8u) * (1.0 / 16777216.0);
}

fn cos_dir(n: vec3f, u: vec2f) -> vec3f {
  let r = sqrt(u.x);
  let ph = 2.0 * PI * u.y;
  return basis(n) * vec3f(r * cos(ph), r * sin(ph), sqrt(max(1.0 - u.x, 0.0)));
}

/** A direction uniformly distributed over the sun disc. */
fn sun_sample(u: vec2f) -> vec3f {
  let cmax = inverseSqrt(1.0 + F.sun.w * F.sun.w);
  let ct = 1.0 - u.x * (1.0 - cmax);
  let st = sqrt(max(1.0 - ct * ct, 0.0));
  let ph = 2.0 * PI * u.y;
  return basis(F.sun.xyz) * vec3f(st * cos(ph), st * sin(ph), ct);
}

/** R2 low-discrepancy point n with a Cranley–Patterson shift. */
fn r2(n: u32, shift: vec2f) -> vec2f { return fract(shift + f32(n) * vec2f(0.7548776662, 0.5698402910)); }

struct RC { rays: u32, steps: u32, caps: u32 }

fn ref_sun(o: vec3f, n: vec3f, u: vec2f, c: ptr<function, RC>) -> f32 {
  let l = sun_sample(u);
  let ndl = dot(n, l);
  if (ndl <= 0.0) { return 0.0; }
  (*c).rays += 1u;
  var steps = 0u;
  var caps = 0u;
  let hit = march_ref(o, l, 1e5, &steps, &caps) < INF;
  (*c).steps += steps;
  (*c).caps += caps;
  return select(ndl, 0.0, hit);
}

/** One sample of sky + bounce irradiance at o (normal n). */
fn ref_indirect(o: vec3f, n: vec3f, u: vec2f, seed: ptr<function, u32>, c: ptr<function, RC>) -> mat2x3f {
  var steps = 0u;
  var caps = 0u;
  var sky = vec3f(0.0);
  var bounce = vec3f(0.0);
  let w = cos_dir(n, u);
  (*c).rays += 1u;
  let th = march_ref(o, w, 1e5, &steps, &caps);
  if (th >= INF) {
    sky = PI * sky_radiance(w);
  } else {
    let y = o + w * th;
    let ny = surface_normal(y, 1e-3);
    if (dot(ny, w) < 0.0) {
      let oy = y + ny * (0.002 + 1e-4 * th);
      var ey = F.sun_e.rgb * ref_sun(oy, ny, vec2f(rnd(seed), rnd(seed)), c);
      let w2 = cos_dir(ny, vec2f(rnd(seed), rnd(seed)));
      (*c).rays += 1u;
      if (march_ref(oy, w2, 1e5, &steps, &caps) >= INF) { ey += PI * sky_radiance(w2); }
      bounce = albedo(y, ny, surface_id(y)) * ey;
    }
  }
  (*c).steps += steps;
  (*c).caps += caps;
  return mat2x3f(sky, bounce);
}

/** Pixel of the reference's indirect grid (stride rf.z >> 16) for grid cell c. */
fn grid_px(c: vec2u, stride: u32) -> vec2u { return c * stride + stride / 4u; }

/** Adds one sample pair (one to A, one to B) to every pixel of one tile: every pixel for the sun,
 *  every grid pixel for the indirect light. F.rf: x pair index, y seed, z (stride << 16), w tile
 *  origin (x low 16 bits, y high), in grid cells for the indirect mode. */
@compute @workgroup_size(8, 8)
fn ref_trace(@builtin(global_invocation_id) gid0: vec3u) {
  let cell = gid0.xy + vec2u(F.rf.w & 0xffffu, F.rf.w >> 16u);
  let stride = max(F.rf.z >> 16u, 1u);
  var gid = cell;
  if (REF_MODE == 1u) { gid = grid_px(cell, stride); }
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let px = vec2i(gid);
  let t = textureLoad(g_t, px, 0).r;
  if (t <= 0.0) { return; }
  let idx = gid.y * F.dims.x + gid.x;
  let p = F.eye.xyz + px_dir(gid) * t;
  let n = textureLoad(g_n, px, 0).xyz;
  let o = p + n * surf_bias(t);
  let ns = F.rf.x;
  let hA = ihash(idx * 4u + F.rf.y);
  let hB = ihash(idx * 4u + 2u + F.rf.y);
  let shA = vec2f(u01(hA), u01(ihash(hA)));
  let shB = vec2f(u01(hB), u01(ihash(hB)));
  var c = RC(0u, 0u, 0u);
  if (REF_MODE == 0u) {
    // Stratified over the disc: the two halves use independent shifts of the same R2 sequence.
    let sa = ref_sun(o, n, r2(ns, shA), &c);
    let sb = ref_sun(o, n, r2(ns, shB), &c);
    acc0[idx].w += sa;
    acc2[idx].w += sb;
  } else {
    var seed = ihash(idx ^ ihash(ns * 2u + F.rf.y * 7919u));
    let ra = ref_indirect(o, n, r2(ns, fract(shA * 7.31)), &seed, &c);
    seed = ihash(seed ^ 0x9e3779b9u);
    let rb = ref_indirect(o, n, r2(ns, fract(shB * 7.31)), &seed, &c);
    acc0[idx] += vec4f(ra[0], 0.0);
    acc1[idx] += vec4f(ra[1], 1.0);
    acc2[idx] += vec4f(rb[0], 0.0);
    acc3[idx] += vec4f(rb[1], 1.0);
  }
  stat(S_REF_RAYS, c.rays);
  stat(S_REF_STEPS, c.steps / 16u);
  stat(S_REF_CAPS, c.caps);
}

fn ref_ind_at(i: u32, half: u32) -> mat2x4f {
  var s0 = vec4f(0.0);
  var s1 = vec4f(0.0);
  if (half != 2u) { s0 += acc0[i]; s1 += acc1[i]; }
  if (half != 1u) { s0 += acc2[i]; s1 += acc3[i]; }
  return mat2x4f(s0, s1);
}

/** Resolves the reference to the same display path as the real-time composite. flags.w picks the
 *  samples (0 both halves, 1 A, 2 B), flags.y the view, rf.x the sun pairs accumulated, rf.z the
 *  grid stride (<< 16). Grid pixels show their own indirect estimate; other pixels reconstruct it
 *  from the four surrounding grid pixels with depth and normal weights (display only). */
@compute @workgroup_size(8, 8)
fn ref_resolve(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let px = vec2i(gid.xy);
  let t = textureLoad(g_t, px, 0).r;
  let d = px_dir(gid.xy);
  if (t <= 0.0) { textureStore(out_w, px, vec4f(background(d), 1.0)); return; }
  let idx = gid.y * F.dims.x + gid.x;
  let half = F.flags.w;
  let n = textureLoad(g_n, px, 0).xyz;
  let a = textureLoad(g_a, px, 0).rgb;
  let nsun = f32(F.rf.x) * select(1.0, 2.0, half == 0u);
  var sun = 0.0;
  if (half != 2u) { sun += acc0[idx].w; }
  if (half != 1u) { sun += acc2[idx].w; }
  let stride = max(F.rf.z >> 16u, 1u);
  let off = stride / 4u;
  var sky = vec3f(0.0);
  var bounce = vec3f(0.0);
  if ((gid.x % stride) == off && (gid.y % stride) == off) {
    let s = ref_ind_at(idx, half);
    sky = s[0].rgb / max(s[1].w, 1.0);
    bounce = s[1].rgb / max(s[1].w, 1.0);
  } else {
    let g = (vec2f(gid.xy) - f32(off)) / f32(stride);
    let c0 = vec2i(floor(g));
    let f = g - vec2f(c0);
    let hi = vec2i((F.dims.xy - off - 1u) / stride);
    var ws = 0.0;
    var nd = INF;
    var near = mat2x4f(vec4f(0.0), vec4f(0.0));
    for (var k = 0; k < 4; k++) {
      let c = clamp(c0 + vec2i(k & 1, k >> 1), vec2i(0), hi);
      let q = vec2u(c) * stride + off;
      let tq = textureLoad(g_t, vec2i(q), 0).r;
      if (tq <= 0.0) { continue; }
      let s = ref_ind_at(q.y * F.dims.x + q.x, half);
      let e = mat2x4f(s[0] / max(s[1].w, 1.0), s[1] / max(s[1].w, 1.0));
      let dz = abs(tq - t);
      if (dz < nd) { nd = dz; near = e; }
      let bw = select(1.0 - f.x, f.x, (k & 1) == 1) * select(1.0 - f.y, f.y, k >= 2);
      let nw = pow(max(dot(n, textureLoad(g_n, vec2i(q), 0).xyz), 0.0), 8.0);
      let w = bw * nw * exp(-dz / (0.02 * t + 0.02)) + 1e-5 * bw;
      sky += w * e[0].rgb;
      bounce += w * e[1].rgb;
      ws += w;
    }
    if (ws < 1e-4) { sky = near[0].rgb; bounce = near[1].rgb; } else { sky /= ws; bounce /= ws; }
  }
  textureStore(out_w, px, vec4f(shade_px(a, t, d, F.sun_e.rgb * (sun / max(nsun, 1.0)), sky, bounce), 1.0));
}
