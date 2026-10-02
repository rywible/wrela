// Shared by every kernel: the frame uniform, buffer layouts, noise, primitives, ray intersections.
// No bindings here except the names other files declare (F, chars, ...).

struct Frame {
  eye: vec4f,        // xyz camera; w: a pixel's angular size (radians per pixel at distance 1)
  cf: vec4f,         // camera forward
  cr: vec4f,         // camera right × tan(half fov x)
  cu: vec4f,         // camera up × tan(half fov y)
  sun: vec4f,        // xyz toward the sun; w: time (s)
  lc: vec4f,         // light grid: xyz centre, w half-extent (m)
  lr: vec4f,         // light grid right axis
  lu: vec4f,         // light grid up axis
  dims: vec4u,       // width, height, tiles across, tiles down
  grid: vec4u,       // light cells per side, characters, patches, segments
  misc: vec4u,       // strands, segments per strand, sheets, band row offset
  wind: vec4f,       // xyz wind (m/s); w: strand radius at the root (m)
  viewproj: mat4x4f,
  lightvp: mat4x4f,
  bins: vec4u,       // x: light region start (= tiles), y: list capacity, z: flags, w: hair particle base
  hair: vec4u,       // x: hair guides, y: particles per guide, z: cloth particles, w: unused
}

const INF: f32 = 1e30;
const BIG: f32 = 1e6;
const PI: f32 = 3.14159265;
const NEAR_Z: f32 = 0.05;      // the raster projection's near and far planes (main.js)
const FAR_Z: f32 = 1000.0;
const SALT: f32 = 0.0;          // replaced per run when measuring cold pipeline creation

// Characters (scene.js writeChar): CS vec4s each.
const CS: u32 = 36u;
const C_LO: u32 = 0u;           // bound min xyz, w: blend radius
const C_HI: u32 = 1u;           // bound max xyz, w: 1 for the hero
const C_GARMENT: u32 = 2u;
const C_TROUSERS: u32 = 3u;
const C_SKIN: u32 = 4u;
const C_HAIR: u32 = 5u;         // rgb, w: 1 if the scalp has hair
const C_FWD: u32 = 6u;
const C_RIGHT: u32 = 7u;
const C_PARTS: u32 = 8u;        // 14 × (a.xyz r1, b.xyz r2)
const NPARTS: u32 = 14u;
const P_HEAD: u32 = 3u;

// Collision groups and kinds (scene.js).
const G_NONE: u32 = 0xffffu;
const G_TOWER: u32 = 0xfffeu;
const KIND_CLOTHES: u32 = 0u;
const KIND_CAPE: u32 = 1u;
const KIND_BANNER: u32 = 2u;
const KIND_HAIR: u32 = 3u;
const KIND_FLAG: u32 = 4u;

// Binning categories.
const CAT_CHAR: u32 = 0u;
const CAT_PATCH: u32 = 1u;
const CAT_SEG: u32 = 2u;
const NCAT: u32 = 3u;

// prims buffer: [hair grid (2)] [patches (PATCH_STRIDE each)] [strand bounds (2 each)] [patch info (1 each)]
const PR_HAIR: u32 = 0u;
const PR_PATCH: u32 = 2u;
const PATCH_STRIDE: u32 = 3u;   // aabb min (w: n·c), aabb max (w: slab half-width), normal (w: unused)
fn pr_strand(i: u32) -> u32 { return PR_PATCH + PATCH_STRIDE * F.grid.z + 2u * i; }
fn pr_pinfo(i: u32) -> u32 { return PR_PATCH + PATCH_STRIDE * F.grid.z + 2u * F.misc.x + i; }

// Cloth: half its thickness (the field is distance to the mid-surface minus this).
const CLOTH_H: f32 = 0.004;

// Hair density grid.
// Hair density grid: fine for volume hair, coarse where hair only casts shadows (main.js GRIDS).
override HDX: u32 = 64u;
override HDY: u32 = 96u;
override HDZ: u32 = 64u;
fn hd() -> vec3u { return vec3u(HDX, HDY, HDZ); }
const HCOARSE: u32 = 4u;        // skip grid: one cell per 4³ voxels
override HLIGHT_DIV: u32 = 2u;  // transmittance grid: density grid / this (both grids: 32×48×32)

// Tower (scene.js TOWER).
const TOWER_C: vec2f = vec2f(3.0, 16.0);
const TOWER_R: f32 = 3.4;
const TOWER_TOP: f32 = 13.0;

// ---- hashing and noise -------------------------------------------------------------------------------

fn hash_u(x: u32) -> u32 {
  var h = x * 747796405u + 2891336453u;
  h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
  return (h >> 22u) ^ h;
}
fn hash_f(x: u32) -> f32 { return f32(hash_u(x)) * (1.0 / 4294967295.0); }
fn hash2(c: vec2i) -> f32 { let u = bitcast<vec2u>(c); return hash_f((u.x * 1597334677u) ^ (u.y * 3812015801u + 0x9e3779b9u)); }

fn lattice(c: vec3i) -> f32 {
  let u = bitcast<vec3u>(c);
  return f32(hash_u(u.x + hash_u(u.y + hash_u(u.z)))) * (2.0 / 4294967295.0) - 1.0;
}

fn noise3(x: vec3f) -> f32 {
  let fl = floor(x);
  let i = vec3i(fl);
  let f = x - fl;
  let u = f * f * (3.0 - 2.0 * f);
  return mix(mix(mix(lattice(i), lattice(i + vec3i(1, 0, 0)), u.x),
                 mix(lattice(i + vec3i(0, 1, 0)), lattice(i + vec3i(1, 1, 0)), u.x), u.y),
             mix(mix(lattice(i + vec3i(0, 0, 1)), lattice(i + vec3i(1, 0, 1)), u.x),
                 mix(lattice(i + vec3i(0, 1, 1)), lattice(i + vec3i(1, 1, 1)), u.x), u.y), u.z);
}

fn noise2(x: vec2f) -> f32 {
  let fl = floor(x);
  let i = vec2i(fl);
  let f = x - fl;
  let u = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash2(i), hash2(i + vec2i(1, 0)), u.x), mix(hash2(i + vec2i(0, 1)), hash2(i + vec2i(1, 1)), u.x), u.y);
}

fn fbm2(x: vec2f) -> f32 {
  return 0.5 * noise2(x) + 0.25 * noise2(x * 2.03 + 7.1) + 0.125 * noise2(x * 4.01 + 3.3) + 0.0625 * noise2(x * 8.05 + 1.7);
}

// ---- primitives ------------------------------------------------------------------------------------------

fn round_cone_d(p: vec3f, a: vec3f, b: vec3f, r1: f32, r2: f32) -> f32 {
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

/** Round cone distance and gradient (spike 02). */
fn round_cone_g(p: vec3f, a: vec3f, b: vec3f, r1: f32, r2: f32) -> vec4f {
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
  if (sign(z) * a2 * z2 > k) {
    let v = p - b;
    let l = max(length(v), 1e-9);
    return vec4f(l - r2, v / l);
  }
  if (sign(y) * a2 * y2 < k) {
    let l = max(length(pa), 1e-9);
    return vec4f(l - r1, pa / l);
  }
  let len = sqrt(l2);
  let u = ba / len;
  let s = rr / len;
  let perp = pa - u * dot(pa, u);
  let g = sqrt(max(1.0 - s * s, 0.0)) * perp / max(length(perp), 1e-9) + s * u;
  return vec4f((sqrt(x2 * a2 * il2) + y * rr) * il2 - r1, g);
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
  let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
  return mix(b, a, h) - k * h * (1.0 - h);
}

fn dot2(v: vec3f) -> f32 { return dot(v, v); }

/** Unsigned distance to a triangle (Quílez), guarded for degenerate triangles. */
fn ud_tri(p: vec3f, a: vec3f, b: vec3f, c: vec3f) -> f32 {
  let ba = b - a; let pa = p - a;
  let cb = c - b; let pb = p - b;
  let ac = a - c; let pc = p - c;
  let nor = cross(ba, ac);
  let inside = sign(dot(cross(ba, nor), pa)) + sign(dot(cross(cb, nor), pb)) + sign(dot(cross(ac, nor), pc));
  if (inside < 2.0) {
    return sqrt(min(min(
      dot2(ba * clamp(dot(ba, pa) / max(dot2(ba), 1e-14), 0.0, 1.0) - pa),
      dot2(cb * clamp(dot(cb, pb) / max(dot2(cb), 1e-14), 0.0, 1.0) - pb)),
      dot2(ac * clamp(dot(ac, pc) / max(dot2(ac), 1e-14), 0.0, 1.0) - pc)));
  }
  return sqrt(dot(nor, pa) * dot(nor, pa) / max(dot2(nor), 1e-20));
}

/** Closest point on triangle abc to p, as barycentrics (Ericson). */
fn closest_bary(p: vec3f, a: vec3f, b: vec3f, c: vec3f) -> vec3f {
  let ab = b - a; let ac = c - a; let ap = p - a;
  let d1 = dot(ab, ap); let d2 = dot(ac, ap);
  if (d1 <= 0.0 && d2 <= 0.0) { return vec3f(1.0, 0.0, 0.0); }
  let bp = p - b;
  let d3 = dot(ab, bp); let d4 = dot(ac, bp);
  if (d3 >= 0.0 && d4 <= d3) { return vec3f(0.0, 1.0, 0.0); }
  let vc = d1 * d4 - d3 * d2;
  if (vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0) { let v = d1 / max(d1 - d3, 1e-20); return vec3f(1.0 - v, v, 0.0); }
  let cp = p - c;
  let d5 = dot(ab, cp); let d6 = dot(ac, cp);
  if (d6 >= 0.0 && d5 <= d6) { return vec3f(0.0, 0.0, 1.0); }
  let vb = d5 * d2 - d1 * d6;
  if (vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0) { let w = d2 / max(d2 - d6, 1e-20); return vec3f(1.0 - w, 0.0, w); }
  let va = d3 * d6 - d5 * d4;
  if (va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0) {
    let w = (d4 - d3) / max((d4 - d3) + (d5 - d6), 1e-20);
    return vec3f(0.0, 1.0 - w, w);
  }
  let den = 1.0 / max(va + vb + vc, 1e-20);
  let v = vb * den; let w = vc * den;
  return vec3f(1.0 - v - w, v, w);
}

/** Ray vs triangle (Möller–Trumbore). Returns (t, u, v) with t = INF on a miss. */
fn ray_tri(o: vec3f, d: vec3f, a: vec3f, b: vec3f, c: vec3f) -> vec3f {
  let e1 = b - a; let e2 = c - a;
  let pv = cross(d, e2);
  let det = dot(e1, pv);
  if (abs(det) < 1e-12) { return vec3f(INF, 0.0, 0.0); }
  let inv = 1.0 / det;
  let tv = o - a;
  let u = dot(tv, pv) * inv;
  if (u < 0.0 || u > 1.0) { return vec3f(INF, 0.0, 0.0); }
  let qv = cross(tv, e1);
  let v = dot(d, qv) * inv;
  if (v < 0.0 || u + v > 1.0) { return vec3f(INF, 0.0, 0.0); }
  let t = dot(e2, qv) * inv;
  return vec3f(select(INF, t, t > 0.0), u, v);
}

// ---- ray intervals ------------------------------------------------------------------------------------

const EMPTY: vec2f = vec2f(1e30, -1e30);

fn safe_inv(x: f32) -> f32 { return 1.0 / select(x, select(-1e-12, 1e-12, x >= 0.0), abs(x) < 1e-12); }

fn ray_box(o: vec3f, inv: vec3f, lo: vec3f, hi: vec3f) -> vec2f {
  let a = (lo - o) * inv;
  let b = (hi - o) * inv;
  let t0 = max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z));
  let t1 = min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z));
  return select(EMPTY, vec2f(t0, t1), t1 >= max(t0, 0.0));
}

/** Ray vs slab |n·x − c| ≤ h (n unit). */
fn ray_slab(o: vec3f, d: vec3f, n: vec3f, c: f32, h: f32) -> vec2f {
  let dn = dot(d, n);
  let on = dot(o, n) - c;
  if (abs(dn) < 1e-9) { return select(EMPTY, vec2f(-INF, INF), abs(on) <= h); }
  let ta = (-h - on) / dn;
  let tb = (h - on) / dn;
  return vec2f(min(ta, tb), max(ta, tb));
}

fn ray_sphere(o: vec3f, d: vec3f, c: vec3f, r: f32) -> vec2f {
  let oc = o - c;
  let b = dot(oc, d);
  let h = b * b - (dot(oc, oc) - r * r);
  if (h < 0.0) { return EMPTY; }
  let s = sqrt(h);
  return vec2f(-b - s, -b + s);
}

/** Ray vs capsule: the union of its cylinder and its two end spheres (spike 02). */
fn ray_capsule(o: vec3f, d: vec3f, pa: vec3f, pb: vec3f, r: f32) -> vec2f {
  let s0 = ray_sphere(o, d, pa, r);
  let s1 = ray_sphere(o, d, pb, r);
  var res = vec2f(min(s0.x, s1.x), max(s0.y, s1.y));
  let ba = pb - pa;
  let oa = o - pa;
  let baba = dot(ba, ba);
  let bard = dot(ba, d);
  let baoa = dot(ba, oa);
  let a = baba - bard * bard;
  if (a > 1e-9) {
    let b = baba * dot(oa, d) - baoa * bard;
    let c = baba * dot(oa, oa) - baoa * baoa - r * r * baba;
    let h = b * b - a * c;
    if (h >= 0.0) {
      let s = sqrt(h);
      var t0 = (-b - s) / a;
      var t1 = (-b + s) / a;
      // Clip the infinite cylinder to the slab between the caps.
      let y0 = baoa + t0 * bard;
      let y1 = baoa + t1 * bard;
      let in0 = y0 > 0.0 && y0 < baba;
      let in1 = y1 > 0.0 && y1 < baba;
      if (in0) { res.x = min(res.x, t0); res.y = max(res.y, t0); }
      if (in1) { res.x = min(res.x, t1); res.y = max(res.y, t1); }
    }
  }
  return res;
}

/** Closest approach between a ray (o + d t, t ≥ 0) and a segment a–b: (distance, t, s ∈ [0,1]). */
fn ray_segment(o: vec3f, d: vec3f, a: vec3f, b: vec3f) -> vec3f {
  let ba = b - a;
  let oa = a - o;
  let bb = dot(ba, ba);
  let bd = dot(ba, d);
  let den = bb - bd * bd;
  var s = 0.0;
  if (den > 1e-12) { s = clamp((bd * dot(oa, d) - dot(oa, ba)) / den, 0.0, 1.0); }
  let q = a + ba * s;
  let t = max(dot(q - o, d), 0.0);
  // One refinement: re-project the ray point onto the segment.
  s = clamp(dot(o + d * t - a, ba) / max(bb, 1e-12), 0.0, 1.0);
  let q2 = a + ba * s;
  let t2 = max(dot(q2 - o, d), 0.0);
  return vec3f(length(o + d * t2 - q2), t2, s);
}

/** Particle index of patch-local vertex (i, j); rows and columns past the patch's last quad repeat it. */
fn patch_vertex(inf: vec4u, i: u32, j: u32) -> u32 {
  let nu = inf.y & 0xffffu;
  let wrap = (inf.y >> 16u) & 1u;
  let qu = inf.w & 0xffu;
  let qv = (inf.w >> 8u) & 0xffu;
  var u = (inf.z & 0xffffu) + min(i, qu);
  let v = (inf.z >> 16u) + min(j, qv);
  if (wrap == 1u && u >= nu) { u -= nu; }
  return inf.x + v * nu + u;
}

fn patch_sheet(inf: vec4u) -> u32 { return inf.w >> 16u; }

/** Hard occlusion by the tower, as its bounding cylinder (shadows on the hair: hair_light, references). */
fn tower_occlusion(p: vec3f, l: vec3f) -> f32 {
  let r = TOWER_R + 0.28;
  let o = p.xz - TOWER_C;
  let a = dot(l.xz, l.xz);
  if (a < 1e-9) { return 1.0; }
  let b = dot(o, l.xz);
  let c = dot(o, o) - r * r;
  let h = b * b - a * c;
  if (h < 0.0) { return 1.0; }
  let s = sqrt(h);
  let t0 = (-b - s) / a;
  let t1 = (-b + s) / a;
  if (t1 < 0.0) { return 1.0; }
  let y0 = p.y + l.y * max(t0, 0.0);
  return select(1.0, 0.0, y0 < 14.1);
}

// ---- tone and sky -----------------------------------------------------------------------------------

fn tonemap(x: vec3f) -> vec3f {
  let c = x * 0.8;
  return clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), vec3f(0.0), vec3f(1.0));
}

const SUN_COL: vec3f = vec3f(3.3, 2.75, 2.1);
const HAZE: vec3f = vec3f(0.62, 0.62, 0.60);

fn sky(d: vec3f) -> vec3f {
  let up = clamp(d.y, -0.2, 1.0);
  let zen = vec3f(0.18, 0.34, 0.66);
  let hor = vec3f(0.66, 0.66, 0.62);
  var c = mix(hor, zen, pow(max(up, 0.0), 0.55));
  let s = max(dot(d, F.sun.xyz), 0.0);
  c += vec3f(1.0, 0.75, 0.45) * (0.25 * pow(s, 8.0) + 0.6 * pow(s, 64.0));
  c += vec3f(30.0, 26.0, 20.0) * smoothstep(0.9997, 0.9999, s);
  // Clouds: fbm on a plane high above, faded toward the horizon (sky and clouds aren't this spike's question).
  if (d.y > 0.0) {
    let uv = d.xz / (d.y + 0.08) * 0.9 + vec2f(F.sun.w * 0.004, 0.0);
    let n = fbm2(uv * 1.3) + 0.5 * fbm2(uv * 3.1 + 2.0);
    let cov = smoothstep(0.55, 0.95, n) * smoothstep(0.0, 0.25, d.y);
    let lit = 0.75 + 0.35 * pow(s, 4.0);
    c = mix(c, vec3f(0.95, 0.93, 0.9) * lit, cov * 0.85);
  }
  // Low hills on the horizon.
  let az = atan2(d.z, d.x);
  let hill = 0.025 + 0.03 * noise2(vec2f(az * 3.0, 1.0)) + 0.015 * noise2(vec2f(az * 9.0, 4.0));
  if (d.y < hill) { c = mix(vec3f(0.30, 0.36, 0.34), hor, 0.55); }
  return c;
}

fn fog(c: vec3f, t: f32, d: vec3f) -> vec3f {
  let f = 1.0 - exp(-max(t - 6.0, 0.0) * 0.0045);
  return mix(c, mix(HAZE, sky(d), 0.3), f);
}
