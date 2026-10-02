// Shared by every module: the frame uniform, the pose buffer layout, noise, the SDF helpers, and
// the environment (sky, ground, lighting). Each entry file declares `pose` itself, because its
// access mode differs (read in bin/trace, read_write in refit).

struct Frame {
  eye: vec4f,       // xyz: camera; w: a pixel's angular size (radians per pixel at distance 1)
  cf: vec4f,        // camera forward
  cr: vec4f,        // camera right × tan(half fov x)
  cu: vec4f,        // camera up × tan(half fov y)
  sun: vec4f,       // xyz: direction toward the sun; w: time (s)
  lc: vec4f,        // light grid: xyz centre, w half-extent (m)
  lr: vec4f,        // light grid: right axis
  lu: vec4f,        // light grid: up axis
  vol: vec4f,       // AO volume grid: xyz origin, w cell size (m)
  dims: vec4u,      // width, height, tiles across, tiles down
  grid: vec4u,      // x: light cells per side; y: sample index (reference); z: samples per pixel; w: flags
  vdims: vec4u,     // AO volume grid cells per axis; w: enabled part mask
  fur: vec4f,       // x: shell depth inside the surface, y: shell height outside, z: strand tile (m), w: bound margin offset
  root: vec4f,      // xyz: the wolf's root (world); w: unused
}

@group(0) @binding(0) var<uniform> F: Frame;

// ---- pose buffer layout (vec4s), written by wolf.js; joint-part boxes are refit on the GPU -------
const NB: u32 = 22u;              // bones
const NP: u32 = 22u;              // parts (one per bone in the rigid decomposition)
const P_BINV: u32 = 0u;           // bone b, world → rest: 3 rows at 3b
const P_BFWD: u32 = 66u;          // bone b, rest → world: 3 rows at 66 + 3b
const P_HDR: u32 = 132u;          // part p header: 2 vec4s at 132 + 2p
                                  //   [0] n_obb, joint (−1: rigid), march Lipschitz factor, fold k
                                  //   [1] eval bone, side (1: right, mirrored), base fn, margin
const P_OBB: u32 = 176u;          // part p box o: 4 vec4s at 176 + 12p + 4o
                                  //   (centre, 0), (x axis / hx, hx), (y axis / hy, hy), (z axis / hz, hz)
const P_JNT: u32 = 440u;          // joint j: 8 vec4s at 440 + 8j (see wolf.js)
const NJ: u32 = 5u;
const POSE_VEC4S: u32 = 480u;

const INF: f32 = 1e30;
const BIG: f32 = 1e6;
const PI: f32 = 3.14159265;
const TAU: f32 = 6.28318531;
const SALT: f32 = 0.0;            // replaced per run when measuring cold pipeline creation

// ---- SDF helpers (fieldview's lib.wgsl, copied) -------------------------------------------------

fn sd_sphere(p: vec3f, r: f32) -> f32 { return length(p) - r; }

fn sd_box(p: vec3f, half: vec3f) -> f32 {
  let q = abs(p) - half;
  return length(max(q, vec3f(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn sd_round_cone(p: vec3f, a: vec3f, b: vec3f, r1: f32, r2: f32) -> f32 {
  let ba = b - a;
  let l2 = dot(ba, ba);
  let rr = r1 - r2;
  let a2 = l2 - rr * rr;
  let il2 = 1.0 / l2;
  let pa = p - a;
  let y = dot(pa, ba);
  let z = y - l2;
  let xv = pa * l2 - ba * y;
  let x2 = dot(xv, xv);
  let y2 = y * y * l2;
  let z2 = z * z * l2;
  let k = sign(rr) * rr * rr * x2;
  if (sign(z) * a2 * z2 > k) { return sqrt(x2 + z2) * il2 - r2; }
  if (sign(y) * a2 * y2 < k) { return sqrt(x2 + y2) * il2 - r1; }
  return (sqrt(x2 * a2 * il2) + y * rr) * il2 - r1;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
  let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
  return mix(b, a, h) - k * h * (1.0 - h);
}

fn smax(a: f32, b: f32, k: f32) -> f32 { return -smin(-a, -b, k); }

fn rot_x(p: vec3f, a: f32) -> vec3f { let c = cos(a); let s = sin(a); return vec3f(p.x, c * p.y - s * p.z, s * p.y + c * p.z); }
fn rot_y(p: vec3f, a: f32) -> vec3f { let c = cos(a); let s = sin(a); return vec3f(c * p.x + s * p.z, p.y, -s * p.x + c * p.z); }
fn rot_z(p: vec3f, a: f32) -> vec3f { let c = cos(a); let s = sin(a); return vec3f(c * p.x - s * p.y, s * p.x + c * p.y, p.z); }
fn mirror_x(p: vec3f) -> vec3f { return vec3f(abs(p.x), p.y, p.z); }

// Value noise in [-1, 1] and fbm (fieldview's).
fn fv_hash(c: vec3i) -> f32 {
  var h = bitcast<u32>(c.x) * 747796405u + bitcast<u32>(c.y) * 2891336453u + bitcast<u32>(c.z) * 277803737u;
  h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
  h = (h >> 22u) ^ h;
  return f32(h) * (2.0 / 4294967295.0) - 1.0;
}

fn noise3(x: vec3f) -> f32 {
  let fl = floor(x);
  let i = vec3i(fl);
  let f = x - fl;
  let u = f * f * (3.0 - 2.0 * f);
  return mix(mix(mix(fv_hash(i), fv_hash(i + vec3i(1, 0, 0)), u.x),
                 mix(fv_hash(i + vec3i(0, 1, 0)), fv_hash(i + vec3i(1, 1, 0)), u.x), u.y),
             mix(mix(fv_hash(i + vec3i(0, 0, 1)), fv_hash(i + vec3i(1, 0, 1)), u.x),
                 mix(fv_hash(i + vec3i(0, 1, 1)), fv_hash(i + vec3i(1, 1, 1)), u.x), u.y), u.z);
}

fn fbm3(x: vec3f, octaves: i32) -> f32 {
  var s = 0.0;
  var a = 0.5;
  var q = x;
  for (var o = 0; o < octaves; o++) {
    s += a * noise3(q);
    q = q * 2.03 + vec3f(1.7, 9.2, 3.1);
    a *= 0.5;
  }
  return s;
}

fn hash2(p: vec2u) -> f32 {
  var h = p.x * 747796405u + p.y * 2891336453u + 12345u;
  h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
  h = (h >> 22u) ^ h;
  return f32(h) / 4294967295.0;
}

// ---- environment ---------------------------------------------------------------------------------
// Late-afternoon light over a forest floor. The same functions shade the traced and raster paths.

const SUN_COL = vec3f(3.3, 2.75, 2.1);
const SKY_TOP = vec3f(0.22, 0.36, 0.62);
const SKY_HOR = vec3f(0.62, 0.64, 0.64);
const HAZE = vec3f(0.50, 0.55, 0.58);

fn tonemap(x: vec3f) -> vec3f {
  let c = x * 0.68;
  return clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), vec3f(0.0), vec3f(1.0));
}

fn sky(d: vec3f) -> vec3f {
  let up = clamp(d.y, 0.0, 1.0);
  var c = mix(SKY_HOR, SKY_TOP, pow(up, 0.55));
  let sd = max(dot(d, F.sun.xyz), 0.0);
  c += vec3f(1.0, 0.75, 0.5) * (0.25 * pow(sd, 6.0) + 6.0 * pow(sd, 900.0));
  return c;
}

// Sky light arriving at a surface with normal n (cheap hemisphere: sky above, warm bounce below).
fn ambient(n: vec3f) -> vec3f {
  return mix(vec3f(0.10, 0.08, 0.05), vec3f(0.36, 0.44, 0.58), 0.5 + 0.5 * n.y);
}

/** Forest floor: moss and leaf litter. Octaves above the pixel footprint fade out. */
fn ground_albedo(p: vec3f, fp: f32) -> vec3f {
  let q = vec3f(p.x, 0.0, p.z);
  let big = fbm3(q * 0.7, 3);
  let mid = noise3(q * 6.0);
  let leaf = noise3(q * 23.0) * (1.0 - smoothstep(0.006, 0.02, fp));
  let fine = noise3(q * 61.0) * (1.0 - smoothstep(0.002, 0.008, fp));
  let moss = vec3f(0.030, 0.046, 0.014);
  let litter = vec3f(0.080, 0.050, 0.026);
  let dry = vec3f(0.120, 0.085, 0.048);
  let soil = vec3f(0.032, 0.024, 0.016);
  var c = mix(moss, litter, smoothstep(-0.25, 0.2, big + 0.3 * mid));
  c = mix(c, dry, 0.5 * smoothstep(0.25, 0.6, leaf) * smoothstep(-0.1, 0.3, big));
  c = mix(c, soil, 0.4 * smoothstep(0.3, 0.7, mid));
  return c * (0.8 + 0.2 * fine);
}

fn fog(c: vec3f, d: vec3f, t: f32) -> vec3f {
  let f = 1.0 - exp(-max(t - 4.0, 0.0) * 0.03);
  return mix(c, mix(HAZE, sky(d), 0.4), f);
}

fn shade_ground(p: vec3f, d: vec3f, t: f32, sh: f32, ao: f32) -> vec3f {
  let n = vec3f(0.0, 1.0, 0.0);
  let alb = ground_albedo(p, t * F.eye.w);
  let ndl = max(F.sun.y, 0.0);
  let c = alb * (SUN_COL * ndl * sh + ambient(n) * ao);
  return fog(c, d, t);
}

// ---- fur shading (Kajiya-Kay with two shifted lobes, after Scheuermann's hair model) ------------

struct FurLight { diff: f32, spec1: f32, spec2: f32, lam: f32 }

fn fur_light(T: vec3f, n: vec3f, v: vec3f) -> FurLight {
  let l = F.sun.xyz;
  let h = normalize(l + v);
  let t1 = normalize(T + n * 0.10);       // primary lobe shifted toward the tip
  let t2 = normalize(T - n * 0.12);       // secondary (coloured) lobe toward the root
  let th1 = dot(t1, h);
  let th2 = dot(t2, h);
  let tl = dot(T, l);
  var o: FurLight;
  o.diff = sqrt(max(1.0 - tl * tl, 0.0));
  o.spec1 = pow(sqrt(max(1.0 - th1 * th1, 0.0)), 90.0);
  o.spec2 = pow(sqrt(max(1.0 - th2 * th2, 0.0)), 18.0);
  o.lam = clamp((dot(n, l) + 0.35) / 1.35, 0.0, 1.0);   // wrapped Lambert keeps the body's form readable
  return o;
}

/** A furred surface seen from far enough that strands are sub-pixel (fur LOD 2, and the raster mesh). */
fn shade_fur_surface(alb: vec3f, n: vec3f, T: vec3f, v: vec3f, sh: f32, ao: f32) -> vec3f {
  let fl = fur_light(T, n, v);
  let direct = mix(fl.lam, fl.diff * fl.lam, 0.45) * sh;
  let spec = (0.05 * fl.spec1 + 0.10 * fl.spec2 * alb * 4.0) * sh * fl.lam;
  return alb * (SUN_COL * direct + ambient(n) * ao) + SUN_COL * spec;
}
