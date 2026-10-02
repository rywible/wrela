// Shared by every module: the frame uniform, constants and the noise primitives.

struct Level {
  rel: vec4f,     // xyz: the level cube's min corner, relative to the camera (m); w: brick size (m)
  org: vec4i,     // xyz: the cube's min corner in absolute brick coordinates
  bnd: vec4f,     // x: cell (m); y: delta, the cache's error bound (m); z: A, omitted-octave amplitude; w: half-diagonal
}

struct Frame {
  cam: vec4f,     // xyz: camera in shader-world coordinates; w: radians per pixel
  cf: vec4f,      // camera forward
  cr: vec4f,      // camera right × tan(half fov x)
  cu: vec4f,      // camera up × tan(half fov y)
  sun: vec4f,     // xyz: toward the sun
  shift: vec4f,   // xyz: content = shader-world − shift (the precision test); w: view distance (m)
  dims: vec4u,    // width, height, levels, N (bricks per level side)
  misc: vec4f,    // x: finest cell c0 (m); y: time (s); z: atlas slots per row; w: atlas slots per layer
  dbg: vec4u,     // x: debug view (0 shaded, 1 steps, 2 level, 3 analytic evals); y: parts mask
  atl: vec4f,     // xyz: 1 / atlas size in texels
  lv: array<Level, 14>,
}

@group(0) @binding(0) var<uniform> F: Frame;

const INF: f32 = 1e30;
const BRICK: f32 = 8.0;               // cells per brick side (9 samples)
const SALT: f32 = 0.0;                // replaced per run when measuring cold pipeline creation

// Parts of the world field (dbg.y masks them; the raster comparison uses terrain only).
const P_TERRAIN: u32 = 1u;
const P_ROCKS: u32 = 2u;
const P_RUINS: u32 = 4u;

// ---- noise ------------------------------------------------------------------------------------

fn hash_u(x: u32) -> u32 {
  var h = x * 747796405u + 2891336453u;
  h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
  return (h >> 22u) ^ h;
}

fn hash2i(i: vec2i, seed: u32) -> u32 {
  let u = bitcast<vec2u>(i);
  return hash_u(u.x + hash_u(u.y + seed));
}

fn lat2(i: vec2i, seed: u32) -> f32 {
  return f32(hash2i(i, seed)) * (2.0 / 4294967295.0) - 1.0;
}

fn lat3(i: vec3i, seed: u32) -> f32 {
  let u = bitcast<vec3u>(i);
  return f32(hash_u(u.x + hash_u(u.y + hash_u(u.z + seed)))) * (2.0 / 4294967295.0) - 1.0;
}

/** 2D value noise in [-1, 1] with its derivative: (value, d/dx, d/dy). Quintic interpolation. */
fn noise2(x: vec2f, seed: u32) -> vec3f {
  let fl = floor(x);
  let i = vec2i(fl);
  let f = x - fl;
  let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  let du = 30.0 * f * f * (f * (f - 2.0) + 1.0);
  let a = lat2(i, seed);
  let b = lat2(i + vec2i(1, 0), seed);
  let c = lat2(i + vec2i(0, 1), seed);
  let d = lat2(i + vec2i(1, 1), seed);
  let k1 = b - a;
  let k2 = c - a;
  let k4 = a - b - c + d;
  return vec3f(a + k1 * u.x + k2 * u.y + k4 * u.x * u.y, du * vec2f(k1 + k4 * u.y, k2 + k4 * u.x));
}

/** 3D value noise in [-1, 1]. */
fn noise3(x: vec3f, seed: u32) -> f32 {
  let fl = floor(x);
  let i = vec3i(fl);
  let f = x - fl;
  let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  let a = lat3(i, seed);
  let b = lat3(i + vec3i(1, 0, 0), seed);
  let c = lat3(i + vec3i(0, 1, 0), seed);
  let d = lat3(i + vec3i(1, 1, 0), seed);
  let e = lat3(i + vec3i(0, 0, 1), seed);
  let g = lat3(i + vec3i(1, 0, 1), seed);
  let h = lat3(i + vec3i(0, 1, 1), seed);
  let k = lat3(i + vec3i(1, 1, 1), seed);
  return mix(mix(mix(a, b, u.x), mix(c, d, u.x), u.y), mix(mix(e, g, u.x), mix(h, k, u.x), u.y), u.z);
}

/** How much of an octave survives a footprint (D-077): fades from 8 to 4 footprints per wavelength,
 *  so 1-sample shading has no detail finer than ~4 pixels (less aliasing, less shimmer). Cooking
 *  passes half a cell as the footprint, i.e. a cell-relative fade from 4 to 2 cells. */
fn octave_weight(fp: f32, wavelength: f32) -> f32 {
  return 1.0 - smoothstep(0.125, 0.25, fp / wavelength);
}

fn ray_dir(px: vec2f) -> vec3f {
  let u = px.x / f32(F.dims.x) * 2.0 - 1.0;
  let v = 1.0 - px.y / f32(F.dims.y) * 2.0;
  return normalize(F.cf.xyz + F.cr.xyz * u + F.cu.xyz * v);
}

fn oct_encode(n: vec3f) -> vec2f {
  let p = n.xz / (abs(n.x) + abs(n.y) + abs(n.z));
  let q = select((1.0 - abs(p.yx)) * select(vec2f(-1.0), vec2f(1.0), p >= vec2f(0.0)), p, n.y >= 0.0);
  return q;
}

fn oct_decode(e: vec2f) -> vec3f {
  var n = vec3f(e.x, 1.0 - abs(e.x) - abs(e.y), e.y);
  let t = max(-n.y, 0.0);
  n.x += select(t, -t, n.x >= 0.0);
  n.z += select(t, -t, n.z >= 0.0);
  return normalize(n);
}
