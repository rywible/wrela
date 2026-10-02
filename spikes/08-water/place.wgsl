// One-time placement, run before any frame: each tree cell, each rock's height, the tower's base,
// and ground heights at probe points (for the cameras). Plus a probe that checks the terrain's
// declared slope bounds and the noise-slope hypothesis by finite differences. Appended to world.wgsl.

@group(0) @binding(0) var<storage, read_write> objs: array<vec4f>;
@group(0) @binding(1) var<storage, read_write> lip: array<atomic<u32>, 8>;
@group(0) @binding(2) var hm_w: texture_storage_2d<r32float, write>;

@group(0) @binding(3) var n3_w: texture_storage_3d<r32float, write>;
@group(0) @binding(4) var n2_w: texture_storage_2d<rgba32float, write>;

const HM_N: u32 = 2048u;
const HM_HALF: f32 = 150.0;

/** Cooks the terrain into the heightmap the tracer marches: texel centres of a 300 m square. */
@compute @workgroup_size(8, 8)
fn bake(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= HM_N || id.y >= HM_N) { return; }
  let xz = (vec2f(id.xy) + 0.5) / f32(HM_N) * (2.0 * HM_HALF) - HM_HALF;
  textureStore(hm_w, id.xy, vec4f(terrain(xz), 0.0, 0.0, 0.0));
}

const ROCK0: u32 = 8192u;
const MAX_ROCKS: u32 = 32u;
const TOWER_SLOT: u32 = 8256u;
const PROBE0: u32 = 8257u;
const NPROBES: u32 = 8u;
const BLOCK0: u32 = 8265u;

/** The tallest tree top in each block of 8×8 cells (−1e9 if none), after place(). */
@compute @workgroup_size(64)
fn place_blocks(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= 64u) { return; }
  let bx = id.x % 8u;
  let bz = id.x / 8u;
  var top = -1e9;
  for (var j = 0u; j < 8u; j++) {
    for (var i = 0u; i < 8u; i++) {
      let c = (bz * 8u + j) * GRID_N + bx * 8u + i;
      let a = objs[2u * c];
      if (a.w > 0.0) { top = max(top, a.y + a.w + 0.6); }
    }
  }
  objs[BLOCK0 + id.x] = vec4f(top, 0.0, 0.0, 0.0);
}

@compute @workgroup_size(64)
fn place(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x;
  if (i < NCELLS) {
    let cell = vec2i(i32(i % GRID_N), i32(i / GRID_N));
    let c = (vec2f(cell) + 0.5) * CELL + GRID_ORIGIN;
    let p = c + (vec2f(h01(cell, 1u), h01(cell, 2u)) - 0.5) * 2.0 * JITTER;
    var a = vec4f(p.x, 0.0, p.y, 0.0);
    var b = vec4f(0.0);
    if (h01(cell, 3u) < tree_density(p)) {
      let s = h01(cell, 5u);
      let v = h01(cell, 6u);
      if (h01(cell, 4u) < birch_share(p)) {
        let h = 7.0 + 5.0 * s;
        a = vec4f(p.x, terrain(p), p.y, h);
        b = vec4f(1.0 + 0.4 * v, 1.0, 0.09 + 0.04 * s, h01(cell, 7u));
      } else {
        let h = 9.0 + 8.5 * s;
        a = vec4f(p.x, terrain(p), p.y, h);
        b = vec4f(min(0.1 * h + 0.35 * v, 1.5), 0.0, 0.11 + 0.011 * h, h01(cell, 7u));
      }
    }
    objs[2u * i] = a;
    objs[2u * i + 1u] = b;
    return;
  }
  let k = i - NCELLS;
  if (k < MAX_ROCKS) {
    // The harness wrote (x, height above ground, z, radius); this sets y.
    let a = objs[ROCK0 + 2u * k];
    if (a.w > 0.0 && objs[ROCK0 + 2u * k + 1u].w < 2.0) { objs[ROCK0 + 2u * k] = vec4f(a.x, terrain(a.xz) + a.y, a.z, a.w); }
    return;
  }
  if (k == MAX_ROCKS) {
    // The tower stands on the lowest ground under its footprint; its foundation goes below.
    var y = terrain(TOWER_XZ);
    for (var j = 0u; j < 12u; j++) {
      let ang = f32(j) * 0.5235988;
      y = min(y, terrain(TOWER_XZ + 3.4 * vec2f(cos(ang), sin(ang))));
    }
    objs[TOWER_SLOT] = vec4f(TOWER_XZ.x, y - 0.1, TOWER_XZ.y, 0.0);
    return;
  }
  let j = k - MAX_ROCKS - 1u;
  if (j < NPROBES) {
    let a = objs[PROBE0 + j];
    objs[PROBE0 + j] = vec4f(a.x, terrain(a.xz), a.z, lake_rn(a.xz));
  }
}

fn in_steep(xz: vec2f) -> bool {
  let q = xz - S_MOUTH;
  let s = dot(q, S_U);
  let l = dot(q, S_V);
  return (s >= S_FALL - 2.6 && s <= S_FALL + 1.6 && abs(l) <= 31.0) || (s >= -7.0 && s <= 47.0 && abs(l) <= 14.5);
}

/** Max |∇terrain| outside (slot 0) and inside (slot 1) the steep boxes, of the cooked heightmap's
 *  bilinear interpolant (what's traced); max |∇noise3| (2) and |∇noise2| (3) at unit frequency.
 *  Floats compared as bits (all positive). */
@compute @workgroup_size(64)
fn probe(@builtin(global_invocation_id) id: vec3u) {
  for (var k = 0u; k < 16u; k++) {
    let cell = vec2i(i32(id.x), i32(k));
    let xz = (vec2f(h01(cell, 11u), h01(cell, 12u)) - 0.5) * 290.0;
    let e = 0.005;
    // The bilinear patch around xz, from texel-centre heights as the bake writes them.
    let texel = 2.0 * HM_HALF / f32(HM_N);
    let gpos = (xz + HM_HALF) / texel - 0.5;
    let c0 = (floor(gpos) + 0.5) * texel - HM_HALF;
    let f = gpos - floor(gpos);
    let h00 = terrain(c0);
    let h10 = terrain(c0 + vec2f(texel, 0.0));
    let h01v = terrain(c0 + vec2f(0.0, texel));
    let h11 = terrain(c0 + vec2f(texel, texel));
    let gx = mix(h10 - h00, h11 - h01v, f.y) / texel;
    let gz = mix(h01v - h00, h11 - h10, f.x) / texel;
    let g = length(vec2f(gx, gz));
    atomicMax(&lip[select(0u, 1u, in_steep(xz))], bitcast<u32>(g));
    let p = vec3f(xz * 0.37, h01(cell, 13u) * 50.0);
    let n3 = vec3f(noise3(p + vec3f(e, 0.0, 0.0)) - noise3(p - vec3f(e, 0.0, 0.0)),
                   noise3(p + vec3f(0.0, e, 0.0)) - noise3(p - vec3f(0.0, e, 0.0)),
                   noise3(p + vec3f(0.0, 0.0, e)) - noise3(p - vec3f(0.0, 0.0, e))) / (2.0 * e);
    atomicMax(&lip[2], bitcast<u32>(length(n3)));
    let n2 = vec2f(noise2(p.xy + vec2f(e, 0.0)) - noise2(p.xy - vec2f(e, 0.0)),
                   noise2(p.xy + vec2f(0.0, e)) - noise2(p.xy - vec2f(0.0, e))) / (2.0 * e);
    atomicMax(&lip[3], bitcast<u32>(length(n2)));
  }
}

// ---- cooked noise: the lattice noise of world.wgsl, made periodic ----------------------------------------

fn wrap3(c: vec3i, per: i32) -> vec3i { return ((c % per) + per) % per; }
fn wrap2(c: vec2i, per: i32) -> vec2i { return ((c % per) + per) % per; }

fn noise3p(x: vec3f, per: i32) -> f32 {
  let fl = floor(x);
  let i = vec3i(fl);
  let f = x - fl;
  let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  let a = lat3(wrap3(i, per));
  let b = lat3(wrap3(i + vec3i(1, 0, 0), per));
  let c = lat3(wrap3(i + vec3i(0, 1, 0), per));
  let d = lat3(wrap3(i + vec3i(1, 1, 0), per));
  let e = lat3(wrap3(i + vec3i(0, 0, 1), per));
  let f1 = lat3(wrap3(i + vec3i(1, 0, 1), per));
  let g = lat3(wrap3(i + vec3i(0, 1, 1), per));
  let h = lat3(wrap3(i + vec3i(1, 1, 1), per));
  return mix(mix(mix(a, b, u.x), mix(c, d, u.x), u.y), mix(mix(e, f1, u.x), mix(g, h, u.x), u.y), u.z);
}

fn noise2p_g(x: vec2f, per: i32) -> vec3f {
  let fl = floor(x);
  let i = vec2i(fl);
  let f = x - fl;
  let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  let du = 30.0 * f * f * (f * (f - 2.0) + 1.0);
  let a = lat2(wrap2(i, per));
  let b = lat2(wrap2(i + vec2i(1, 0), per));
  let c = lat2(wrap2(i + vec2i(0, 1), per));
  let d = lat2(wrap2(i + vec2i(1, 1), per));
  let k = a - b - c + d;
  return vec3f(a + (b - a) * u.x + (c - a) * u.y + k * u.x * u.y, du * vec2f(b - a + k * u.y, c - a + k * u.x));
}

/** 96³ samples of a 16-unit-periodic noise3 (6 per unit). Texel i holds the value at (i + 0.5) / 6. */
@compute @workgroup_size(4, 4, 4)
fn bake_n3(@builtin(global_invocation_id) id: vec3u) {
  if (any(id >= vec3u(96u))) { return; }
  textureStore(n3_w, id, vec4f(noise3p((vec3f(id) + 0.5) / 6.0, 16), 0.0, 0.0, 0.0));
}

/** 512² samples of a 64-unit-periodic noise2 and its gradient (8 per unit). */
@compute @workgroup_size(8, 8)
fn bake_n2(@builtin(global_invocation_id) id: vec3u) {
  if (any(id.xy >= vec2u(512u))) { return; }
  textureStore(n2_w, id.xy, vec4f(noise2p_g((vec2f(id.xy) + 0.5) / 8.0, 64), 0.0));
}
