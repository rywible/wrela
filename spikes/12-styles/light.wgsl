// Per-pixel light inputs from the field: sun shadow, AO, thickness, curvature.
// Appended to common.wgsl + placement + wolf.wgsl + scene.wgsl.

@group(0) @binding(2) var gb: texture_2d<u32>;
@group(0) @binding(3) var light_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(4) var<storage, read_write> stats: array<atomic<u32>, 32>;

override SHADOW_CUT: f32 = 0.002;   // stop once the penumbra estimate is below this (cel: 0.25)
override SH_STEP: f32 = 1.0;        // < 1 for the reference
override SH_MAX: u32 = 160u;
override CURV: bool = false;        // cel: curvature for inner lines
override STATS: bool = false;

const SOFT: f32 = 10.0;             // penumbra sharpness
const SH_TMAX: f32 = 45.0;

const S_SH_MARCHES: u32 = 16u;
const S_SH_STEPS: u32 = 17u;
const S_SH_CAPS: u32 = 18u;

struct Shadow { v: f32, steps: u32, capped: bool }

fn sun_shadow(o: vec3f, fp: f32) -> Shadow {
  let d = F.sun.xyz;
  var r = Shadow(1.0, 0u, false);
  var t = 0.0;
  var ph = INF;
  var ds = 0.0;
  loop {
    if (r.steps >= SH_MAX) { r.capped = true; break; }
    r.steps++;
    let p = o + d * t;
    let gap = p.y - terrain_h(p.x, p.z);
    if (gap > TOP_ALL) { break; }                 // above every occluder
    let trees_on = gap < TREE_TOP;
    // Occluder detail is filtered to the penumbra's width here (t / SOFT, half of it as a footprint),
    // or the receiver's footprint if larger: finer detail can't show in the shadow, and the canopy's
    // displaced "distance" is far from Euclidean, which makes a sampled penumbra minimum noisy.
    // Bounds may stand in for distances only beyond t / SOFT, where the penumbra term is ≥ 1.
    let h = scene(p, max(fp, 0.5 * t / SOFT), max(0.05, t / SOFT), trees_on);
    if (h.d < 5e-4) { r.v = 0.0; break; }
    let ca = closest_approach(ph, h.d, ds);
    r.v = min(r.v, SOFT * ca.x / max(t - ca.y, 1e-3));
    if (r.v < SHADOW_CUT) { r.v = 0.0; break; }
    ph = h.d;
    // denser steps where the penumbra term is active, so the sampled minimum is close to the
    // continuous one; full sphere-tracing steps elsewhere
    let dense = select(1.0, 0.6, SOFT * h.d < 1.5 * t);
    var s = max(h.s * SH_STEP * dense, 0.002);
    if (trees_on) { s = min(s, tree_limit(p, d, tree_index(p))); }
    ds = s;
    t += s;
    if (t > SH_TMAX) { break; }
  }
  return r;
}

/** Four taps along the normal (field AO). Normalized per tap, so open flat ground reads 1. */
fn field_ao(p: vec3f, n: vec3f, fp: f32, ground: bool) -> f32 {
  // a tap at height h is within h of p, so nothing can occlude it if every object is ≥ 2h away:
  // open ground skips the taps (exact)
  if (ground && scene(p, fp, 1.5, true).d > 2.0) { return 1.0; }
  var occ = 0.0;
  var w = 1.0;
  for (var i = 1u; i <= 4u; i++) {
    let hh = 0.06 * f32(i * i) + 0.02;              // 0.08, 0.26, 0.56, 0.98 m
    let q = p + n * hh;
    // ground pixels skip the terrain's own term: it's flat at this scale, and the hit can sit a
    // little below it, which would read as occlusion
    var dd = scene(q, max(fp, 0.05), 1.5, true).d;
    if (!ground) { dd = min(dd, ground_d(q)); }
    occ += w * max(hh - dd, 0.0) / hh;
    w *= 0.75;
  }
  return clamp(1.0 - 0.5 * occ, 0.0, 1.0);
}

@compute @workgroup_size(8, 8)
fn light(@builtin(global_invocation_id) gid0: vec3u) {
  let gid = vec3u(gid0.xy + F.dims.zw, 0u);         // + the tile origin (0 for a whole frame)
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  let g = gb_unpack(textureLoad(gb, vec2i(gid.xy), 0));
  if (g.mat == MAT_SKY) { textureStore(light_out, vec2i(gid.xy), vec4f(1.0, 1.0, 0.0, 0.0)); return; }
  let d = ray_dir(gid.xy);
  let p = F.eye.xyz + d * g.t;
  let n = g.n;
  let fp = g.t * F.eye.w;
  let ndl = dot(n, F.sun.xyz);

  var sh = Shadow(1.0, 0u, false);
  if (ndl > 0.0) {
    sh = sun_shadow(p + n * max(0.01, 3.0 * fp), fp);
  } else if (g.mat == MAT_LEAF) {
    // Facing away, a leaf gets no direct light, only what comes through its own clump
    // (translucency). Two field samples toward the sun stand in for a shadow march: how deep
    // inside foliage the sun-ward points are.
    let a = scene(p + F.sun.xyz * 0.35, fp, 1.0, true).d;
    let b = scene(p + F.sun.xyz * 0.9, fp, 1.0, true).d;
    sh.v = smoothstep(-0.6, 0.1, min(a, b));
  }
  // AO, thickness and the back-face samples work at AO scale (≥ 8 cm): coarse detail is enough
  g_coarse = true;
  let ao = field_ao(p, n, fp, g.mat == MAT_GROUND);
  var th = 0.0;
  if (g.mat == MAT_LEAF) {
    th = clamp(-scene(p - n * 0.45, 0.05, 1.0, true).d / 0.45, 0.0, 1.0);
  }
  g_coarse = false;
  var cv = 0.0;
  if (CURV) {
    // Tetrahedral Laplacian at the crease width: Σ f(p + h k_i) − 4 f(p) ≈ 2 h² Δf, with |h k_i| = δ.
    // cv = −δ Δf: > 0 at concave creases (a fillet tighter than ~2δ reads ~0.5), < 0 at convex edges.
    let delta = F.prm.w * fp;
    let hh = delta * 0.57735;
    let k = vec2f(1.0, -1.0);
    let f0 = scene_d(p, 0.05);
    let s = scene_d(p + k.xyy * hh, 0.05) + scene_d(p + k.yyx * hh, 0.05) +
            scene_d(p + k.yxy * hh, 0.05) + scene_d(p + k.xxx * hh, 0.05);
    cv = -(s - 4.0 * f0) * 1.5 / delta;
  }
  textureStore(light_out, vec2i(gid.xy), vec4f(sh.v, ao, th, cv));

  if (STATS && sh.steps > 0u) {
    atomicAdd(&stats[S_SH_MARCHES], 1u);
    atomicAdd(&stats[S_SH_STEPS], sh.steps);
    if (sh.capped) { atomicAdd(&stats[S_SH_CAPS], 1u); }
  }
}
