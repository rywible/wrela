// The static world marched from the brick cache. Needs `bmap: array<u32>` (read), `atlas:
// texture_3d<f32>` and `samp: sampler` declared by the pass, and the overrides W_STEP, W_EPS,
// WORLD_STEPS, WSHADOW_STEPS (trace.wgsl).

const SOFT: f32 = 32.0;   // soft-shadow sharpness: the penumbra is about distance / SOFT wide

/** Hardware-filtered trilinear sample of one brick; `local` is in voxels, [0, 8]. */
fn brick_sample(slot: u32, local: vec3f) -> vec2f {
  let uvw = (vec3f(slot_origin(slot)) + clamp(local, vec3f(0.0), vec3f(8.0)) + 0.5) / ATLAS;
  return textureSampleLevel(atlas, samp, uvw, 0.0).xy;
}

/** The same trilinear interpolation in f32 from the eight corner texels. Hardware filtering
 *  quantizes its weights (to about 1/256 of a voxel here), which moves a grazing hit by millimetres
 *  along the surface; this is used only to place a converged hit. */
fn brick_exact(slot: u32, local: vec3f) -> f32 {
  let l = clamp(local, vec3f(0.0), vec3f(8.0));
  let i = min(vec3u(floor(l)), vec3u(7u));
  let f = l - vec3f(i);
  let o = vec3i(slot_origin(slot) + i);
  let c000 = textureLoad(atlas, o, 0).x;
  let c100 = textureLoad(atlas, o + vec3i(1, 0, 0), 0).x;
  let c010 = textureLoad(atlas, o + vec3i(0, 1, 0), 0).x;
  let c110 = textureLoad(atlas, o + vec3i(1, 1, 0), 0).x;
  let c001 = textureLoad(atlas, o + vec3i(0, 0, 1), 0).x;
  let c101 = textureLoad(atlas, o + vec3i(1, 0, 1), 0).x;
  let c011 = textureLoad(atlas, o + vec3i(0, 1, 1), 0).x;
  let c111 = textureLoad(atlas, o + vec3i(1, 1, 1), 0).x;
  return mix(mix(mix(c000, c100, f.x), mix(c010, c110, f.x), f.y), mix(mix(c001, c101, f.x), mix(c011, c111, f.x), f.y), f.z);
}

fn cache_exact(p: vec3f) -> f32 {
  let q = p - RMIN;
  if (any(q < vec3f(0.0)) || any(q >= RMAX - RMIN)) { return terrain_sdf(p); }
  let cf = floor(q);
  let e = bmap[cell_index(vec3i(cf))];
  if ((e & BRICK) != 0u) { return brick_exact(e & SLOT_MASK, (q - cf) * VPC); }
  return unpack2x16float(e).x - length(q - cf - 0.5);
}

/** Places a converged brick hit on the exact trilinear surface: a secant through two exact samples
 *  bracketing it, bounded like `refine_hit`. */
fn refine_cache(o: vec3f, d: vec3f, t: f32, eps: f32) -> f32 {
  let ta = max(t - 4.0 * eps, 0.0);
  let da = cache_exact(o + d * ta);
  let db = cache_exact(o + d * t);
  if (da > db && da > 0.0) {
    return clamp(t + db * (t - ta) / (da - db), ta, t + 32.0 * eps);
  }
  return t + db;
}

fn cell_clamped(q: vec3f) -> vec3f {
  return clamp(floor(q), vec3f(0.0), vec3f(f32(RX - 1), f32(RY - 1), f32(RZ - 1)));
}

/** The cached world at p: (distance, disturbed). Outside the region, the terrain. In an empty
 *  cell, a lower bound from its centre's distance. */
fn cache_dg(p: vec3f) -> vec2f {
  let q = p - RMIN;
  if (any(q < vec3f(0.0)) || any(q >= RMAX - RMIN)) { return vec2f(terrain_sdf(p), 0.0); }
  let cf = floor(q);
  let e = bmap[cell_index(vec3i(cf))];
  if ((e & BRICK) != 0u) { return brick_sample(e & SLOT_MASK, (q - cf) * VPC); }
  return vec2f(unpack2x16float(e).x - length(q - cf - 0.5), 0.0);
}

/** A converged world hit, moved onto the surface along the ray: the secant through the last two
 *  samples, which is exact for a plane. At grazing angles a hit a tolerance above the surface is
 *  a long way along the ray (tolerance / sin of the angle), and that shifts surface patterns.
 *  The correction is bounded, so a ray converging beside a silhouette can't jump far. */
fn refine_hit(t: f32, dist: f32, t_prev: f32, d_prev: f32, eps: f32) -> f32 {
  if (d_prev < INF && d_prev > dist && dist > 0.0) {
    return t + clamp(dist * (t - t_prev) / (d_prev - dist), 0.0, 32.0 * eps);
  }
  return t + dist;
}

/** Distance from q (region coordinates) to where the ray leaves cell cf. */
fn cell_exit(q: vec3f, cf: vec3f, inv: vec3f, sgn: vec3f) -> f32 {
  let t = (cf + sgn - q) * inv;
  return min(min(t.x, t.y), t.z);
}

/** Primary march through the cache, from t0 to t1 (the ray's span inside the region).
 *  Empty cells: step to the cell's exit, or further if the centre distance allows. Brick cells:
 *  sphere tracing on the trilinear field, at W_STEP of the sampled distance. */
fn march_cache(o: vec3f, d: vec3f, t0: f32, t1: f32, c: ptr<function, Counters>) -> f32 {
  let inv = vec3f(safe_inv(d.x), safe_inv(d.y), safe_inv(d.z));
  let sgn = step(vec3f(0.0), d);
  var t = t0;
  for (var i = 0u; i < WORLD_STEPS; i++) {
    let q = o + d * t - RMIN;
    let cf = cell_clamped(q);
    let e = bmap[cell_index(vec3i(cf))];
    if ((e & BRICK) != 0u) {
      (*c).w_steps += 1u;
      let dist = brick_sample(e & SLOT_MASK, (q - cf) * VPC).x;
      let eps = max(W_EPS * t * F.eye.w, 2e-4);
      if (dist < eps) { return refine_cache(o, d, t, eps); }
      t += dist * W_STEP;
    } else {
      (*c).w_empty += 1u;
      let dc = unpack2x16float(e).x;
      if (dc <= 0.0) { return t; }
      t += max(dc - length(q - cf - 0.5), cell_exit(q, cf, inv, sgn) + 1e-3);
    }
    if (t > t1) { return INF; }
  }
  // Out of steps: almost always a grazing ray skimming the ground, so call it a hit where it is.
  (*c).w_caps += 1u;
  return t;
}

/** Sun visibility from o through the cache: Quílez's soft shadow (closest-approach estimate) in
 *  brick cells; in empty cells, the conservative centre-distance bound, and a skip. */
fn shadow_cache(o: vec3f, c: ptr<function, Counters>) -> f32 {
  let d = F.sun.xyz;
  let span = ray_box(o, d, RMIN, RMAX);
  if (span.y <= max(span.x, 0.0)) { return 1.0; }
  let inv = vec3f(safe_inv(d.x), safe_inv(d.y), safe_inv(d.z));
  let sgn = step(vec3f(0.0), d);
  (*c).ws_marches += 1u;
  var t = max(span.x, 0.0);
  var res = 1.0;
  var ph = 1e10;
  for (var i = 0u; i < WSHADOW_STEPS; i++) {
    (*c).ws_steps += 1u;
    let q = o + d * t - RMIN;
    let cf = cell_clamped(q);
    let e = bmap[cell_index(vec3i(cf))];
    if ((e & BRICK) != 0u) {
      let h = brick_sample(e & SLOT_MASK, (q - cf) * VPC).x;
      if (h < 1e-3) { return 0.0; }
      let y = h * h / (2.0 * ph);
      let dd = sqrt(max(h * h - y * y, 0.0));
      res = min(res, SOFT * dd / max(t - y, 1e-3));
      ph = h;
      t += max(h * W_STEP, 0.004);
    } else {
      let dc = unpack2x16float(e).x;
      if (dc <= 0.0) { return 0.0; }
      let h = dc - length(q - cf - 0.5);
      res = min(res, SOFT * h / max(t, 1e-3));
      ph = 1e10;
      t += max(h, cell_exit(q, cf, inv, sgn) + 1e-3);
    }
    if (res < 0.004) { return 0.0; }
    if (t > span.y) { return res; }
  }
  (*c).ws_caps += 1u;
  return res;
}
