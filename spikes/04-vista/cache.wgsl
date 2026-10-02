// The clipmap of distance-field bricks: lookup. Shared by march.wgsl and light.wgsl.
//
// Level l is a cube of N³ bricks around the camera, stored toroidally; a brick is 8³ cells, kept as
// 9³ samples in a 3D atlas so trilinear filtering never reads a neighbour. A level is used only
// inside its cube shrunk by one brick, so the slab being re-cooked is never sampled.
// Everything here is camera-relative: no large numbers.

@group(0) @binding(1) var<storage, read> page: array<u32>;
@group(0) @binding(2) var atlas: texture_3d<f32>;
@group(0) @binding(3) var samp: sampler;

const E_UNKNOWN: u32 = 0xFFFFFFFFu;
const E_ALLOC: u32 = 0x80000000u;

struct CacheS {
  d: f32,       // allocated: trilinear distance. empty: a lower bound on the true field at p
  est: f32,     // an estimate of the field at p, for occlusion and penumbrae (not a bound)
  exit: f32,    // empty: distance along the ray to the brick's exit
  level: u32,   // the level used (== levels if p is outside every level)
  kind: u32,    // 0 allocated, 1 empty, 2 outside the cache
}

/** A level that is at or below the finest level containing p (the loop below walks up). */
fn level_guess(p: vec3f) -> u32 {
  let c = max(abs(p.x), max(abs(p.y), abs(p.z)));
  let half0 = f32(F.dims.w) * 0.5 * F.lv[0].rel.w;
  return u32(max(0.0, ceil(log2(max(c / half0, 1e-6)))));
}

/** N is a power of two, so the toroidal wrap is a mask (two's complement handles negatives). */
fn brick_index(L: u32, ab: vec3i) -> u32 {
  let t = vec3u(ab & vec3i(i32(F.dims.w) - 1));
  return ((L * F.dims.w + t.z) * F.dims.w + t.y) * F.dims.w + t.x;
}

fn atlas_coord(slot: u32, f: vec3f) -> vec3f {
  let sx = u32(F.misc.z);
  let sy = u32(F.misc.w);
  let s = vec3u(slot % sx, (slot / sx) % sy, slot / (sx * sy));
  return (vec3f(s) * 9.0 + 0.5 + f * 8.0) * F.atl.xyz;
}

/** Samples the cache at camera-relative p. `dir` is only used for the exit of empty bricks. */
fn cache_at(p: vec3f, dir: vec3f) -> CacheS {
  return cache_from(p, dir, level_guess(p));
}

/** As cache_at, starting the level search at L0 (a primary ray's level never decreases: its
 *  Chebyshev distance from the camera only grows). */
fn cache_from(p: vec3f, dir: vec3f, L0: u32) -> CacheS {
  var L = L0;
  let n = F.dims.z;
  let N = f32(F.dims.w);
  loop {
    if (L >= n) { return CacheS(INF, INF, INF, n, 2u); }
    let lv = F.lv[L];
    let b = lv.rel.w;
    let q = (p - lv.rel.xyz) / b;
    if (any(q < vec3f(1.0)) || any(q >= vec3f(N - 1.0))) { L += 1u; continue; }
    let fq = floor(q);
    let bi = vec3i(fq);
    let e = page[brick_index(L, lv.org.xyz + bi)];
    if (e == E_UNKNOWN) { L += 1u; continue; }
    if ((e & E_ALLOC) != 0u) {
      let d = textureSampleLevel(atlas, samp, atlas_coord(e & 0x7FFFFFFFu, q - fq), 0.0).r;
      return CacheS(d, d, 0.0, L, 0u);
    }
    // Empty: no surface in the brick. Centre-classified bricks also give a bound from the centre's
    // value; bricks the 3³ test proved empty only promise "no surface" (bound 0, skip to the exit).
    let dc = unpack2x16float(e & 0xFFFFu).x;
    let lo = lv.rel.xyz + fq * b;
    let off = length(p - (lo + 0.5 * b));
    var d = dc - off - lv.bnd.z;
    if ((e & 0x10000u) != 0u) { d = 0.0; }
    if (dc < 0.0) { d = min(dc + off, -1e-4); }
    let inv = 1.0 / select(dir, vec3f(1e-9), abs(dir) < vec3f(1e-9));
    let tb = (select(lo, lo + b, dir > vec3f(0.0)) - p) * inv;
    let ex = min(tb.x, min(tb.y, tb.z));
    return CacheS(d, dc, max(ex, 0.0), L, 1u);
  }
}

/** The cache's distance only (allocated or empty), for gradients and occlusion. */
fn cache_d(p: vec3f) -> f32 {
  return cache_at(p, vec3f(0.0, 1.0, 0.0)).est;
}

fn cache_normal(p: vec3f, e: f32) -> vec3f {
  let k0 = vec3f(1.0, -1.0, -1.0);
  let k1 = vec3f(-1.0, -1.0, 1.0);
  let k2 = vec3f(-1.0, 1.0, -1.0);
  let k3 = vec3f(1.0, 1.0, 1.0);
  return normalize(k0 * cache_d(p + k0 * e) + k1 * cache_d(p + k1 * e) + k2 * cache_d(p + k2 * e) + k3 * cache_d(p + k3 * e));
}
