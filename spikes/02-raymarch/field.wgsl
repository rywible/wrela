// The grazer's field, posed, as a tracer needs it: primal, gradient, part bounds and ray intervals.
//
// The primitives and noise are spike 01's field.wgsl, copied (spike 01 stays frozen). What's new:
// - parts are posed rigidly (pose.js); torso and head are evaluated in rest space, cones in world;
// - `mask` is a per-ray PartMask. Dropping a part is exact along a ray that never enters the part's
//   inflated bound (README, "Why leaving parts out is exact");
// - `mode` picks how the torso's displacement is treated: F_BASE leaves it out (spike 01's mesh
//   doesn't carry it either; it only reaches the shading normal), F_BOUND subtracts its bound (a
//   lower bound of the displaced field, so safe to step on), F_FULL evaluates it.

struct Sample { d: f32, g: vec3f, ch: vec2f }

// ---- noise (spike 01) -------------------------------------------------------------------------

fn hash_u(x: u32) -> u32 {
  var h = x * 747796405u + 2891336453u;
  h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
  return (h >> 22u) ^ h;
}

fn lattice(c: vec3i) -> f32 {
  let u = bitcast<vec3u>(c);
  return f32(hash_u(u.x + hash_u(u.y + hash_u(u.z)))) * (2.0 / 4294967295.0) - 1.0;
}

fn noise_d(x: vec3f) -> f32 {
  let fl = floor(x);
  let i = vec3i(fl);
  let f = x - fl;
  let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  let a = lattice(i);
  let b = lattice(i + vec3i(1, 0, 0));
  let c = lattice(i + vec3i(0, 1, 0));
  let d = lattice(i + vec3i(1, 1, 0));
  let e = lattice(i + vec3i(0, 0, 1));
  let f1 = lattice(i + vec3i(1, 0, 1));
  let g = lattice(i + vec3i(0, 1, 1));
  let h = lattice(i + vec3i(1, 1, 1));
  return mix(mix(mix(a, b, u.x), mix(c, d, u.x), u.y), mix(mix(e, f1, u.x), mix(g, h, u.x), u.y), u.z);
}

fn noise_g(x: vec3f) -> vec4f {
  let fl = floor(x);
  let i = vec3i(fl);
  let f = x - fl;
  let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  let du = 30.0 * f * f * (f * (f - 2.0) + 1.0);
  let a = lattice(i);
  let b = lattice(i + vec3i(1, 0, 0));
  let c = lattice(i + vec3i(0, 1, 0));
  let d = lattice(i + vec3i(1, 1, 0));
  let e = lattice(i + vec3i(0, 0, 1));
  let f1 = lattice(i + vec3i(1, 0, 1));
  let g = lattice(i + vec3i(0, 1, 1));
  let h = lattice(i + vec3i(1, 1, 1));
  let k1 = b - a; let k2 = c - a; let k3 = e - a;
  let k4 = a - b - c + d; let k5 = a - c - e + g; let k6 = a - b - e + f1;
  let k7 = -a + b + c - d + e - f1 - g + h;
  let v = a + k1 * u.x + k2 * u.y + k3 * u.z + k4 * u.x * u.y + k5 * u.y * u.z + k6 * u.z * u.x
        + k7 * u.x * u.y * u.z;
  let grad = du * vec3f(k1 + k4 * u.y + k6 * u.z + k7 * u.y * u.z,
                        k2 + k5 * u.z + k4 * u.x + k7 * u.z * u.x,
                        k3 + k6 * u.x + k5 * u.y + k7 * u.x * u.y);
  return vec4f(v, grad);
}

fn octave_weight(fp: f32, freq: f32) -> f32 {
  return 1.0 - smoothstep(0.25, 0.5, fp * freq);
}

fn fbm_d(p: vec3f, fp: f32, misc: vec4f) -> f32 {
  var s = 0.0;
  var a = 1.0;
  var fr = misc.z;
  let octaves = u32(misc.w);
  for (var o = 0u; o < octaves; o++) {
    let w = a * octave_weight(fp, fr);
    if (w <= 0.0) { break; }
    s += w * noise_d(p * fr + f32(o) * 17.0);
    a *= 0.5;
    fr *= 2.0;
  }
  return s;
}

fn fbm_g(p: vec3f, fp: f32, misc: vec4f) -> vec4f {
  var s = vec4f(0.0);
  var a = 1.0;
  var fr = misc.z;
  let octaves = u32(misc.w);
  for (var o = 0u; o < octaves; o++) {
    let w = a * octave_weight(fp, fr);
    if (w <= 0.0) { break; }
    let n = noise_g(p * fr + f32(o) * 17.0);
    s += w * vec4f(n.x, n.yzw * fr);
    a *= 0.5;
    fr *= 2.0;
  }
  return s;
}

/** The displacement's Lipschitz constant, estimated: each live octave adds amplitude × frequency ×
 *  sup|∇noise|, and amplitude × frequency is the same for every octave. NOISE_SLOPE is a
 *  hypothesis checked by the reference comparison, not a proof (a strict bound would be ~6.5). */
const NOISE_SLOPE: f32 = 3.0;
fn fbm_lipschitz(fp: f32, misc: vec4f) -> f32 {
  var w = 0.0;
  var fr = misc.z;
  for (var o = 0u; o < u32(misc.w); o++) {
    w += octave_weight(fp, fr);
    fr *= 2.0;
  }
  return misc.y * misc.z * NOISE_SLOPE * w;
}

// ---- primitives (spike 01) ----------------------------------------------------------------------

fn ellipsoid_d(p: vec3f, r: vec3f) -> f32 {
  let k0 = length(p / r);
  let k1 = max(length(p / (r * r)), 1e-9);
  return k0 * (k0 - 1.0) / k1;
}

fn ellipsoid_g(p: vec3f, r: vec3f) -> vec4f {
  let q2 = p / (r * r);
  let k0 = max(length(p / r), 1e-9);
  let k1 = max(length(q2), 1e-9);
  let dk0 = q2 / k0;
  let dk1 = (q2 / (r * r)) / k1;
  let d = k0 * (k0 - 1.0) / k1;
  return vec4f(d, ((2.0 * k0 - 1.0) * dk0 * k1 - k0 * (k0 - 1.0) * dk1) / (k1 * k1));
}

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

fn smin_d(a: f32, b: f32, k: f32) -> f32 {
  let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
  return mix(b, a, h) - k * h * (1.0 - h);
}

fn smin_g(a: vec4f, b: vec4f, k: f32) -> vec4f {
  let h = clamp(0.5 + 0.5 * (b.x - a.x) / k, 0.0, 1.0);
  return vec4f(mix(b.x, a.x, h) - k * h * (1.0 - h), mix(b.yzw, a.yzw, h));
}

fn smin_s(a: Sample, b: Sample, k: f32) -> Sample {
  let h = clamp(0.5 + 0.5 * (b.d - a.d) / k, 0.0, 1.0);
  return Sample(mix(b.d, a.d, h) - k * h * (1.0 - h), mix(b.g, a.g, h), mix(b.ch, a.ch, h));
}

// ---- posing ---------------------------------------------------------------------------------------

fn iv(base: u32, k: u32) -> vec4f { return inst[base + k]; }

fn to_rest(base: u32, m: u32, p: vec3f) -> vec3f {
  let r0 = inst[base + m]; let r1 = inst[base + m + 1u]; let r2 = inst[base + m + 2u];
  return vec3f(dot(r0.xyz, p) + r0.w, dot(r1.xyz, p) + r1.w, dot(r2.xyz, p) + r2.w);
}

fn dir_rest(base: u32, m: u32, d: vec3f) -> vec3f {
  return vec3f(dot(inst[base + m].xyz, d), dot(inst[base + m + 1u].xyz, d), dot(inst[base + m + 2u].xyz, d));
}

fn dir_world(base: u32, m: u32, g: vec3f) -> vec3f {
  return inst[base + m].xyz * g.x + inst[base + m + 1u].xyz * g.y + inst[base + m + 2u].xyz * g.z;
}

// PartMask bit → cone slot: bits 1–4 are cones 0–3; bits 6–27 are cones 4–25.
fn cone_slot(b: u32) -> u32 { return select(b - 2u, b - 1u, b < 5u); }

// ---- parts (rest space for torso and head, as in spike 01) -------------------------------------------

fn torso_base_d(base: u32, q: vec3f) -> f32 {
  let a = iv(base, I_TORSO);
  let c = iv(base, I_TORSO + 2u);
  return smin_d(ellipsoid_d(q - a.xyz, iv(base, I_TORSO + 1u).xyz),
                ellipsoid_d(q - c.xyz, iv(base, I_TORSO + 3u).xyz), a.w);
}

fn torso_g(base: u32, q: vec3f, fp: f32, misc: vec4f, seed: vec4f) -> vec4f {
  let a = iv(base, I_TORSO);
  let c = iv(base, I_TORSO + 2u);
  let b = smin_g(ellipsoid_g(q - a.xyz, iv(base, I_TORSO + 1u).xyz),
                 ellipsoid_g(q - c.xyz, iv(base, I_TORSO + 3u).xyz), a.w);
  return b + misc.y * fbm_g(q + seed.xyz, fp, misc);
}

fn head_d(base: u32, q: vec3f) -> f32 {
  let e = iv(base, I_HEAD);
  let ma = iv(base, I_HEAD + 2u);
  let mb = iv(base, I_HEAD + 3u);
  return smin_d(ellipsoid_d(q - e.xyz, iv(base, I_HEAD + 1u).xyz), round_cone_d(q, ma.xyz, mb.xyz, ma.w, mb.w), e.w);
}

fn head_g(base: u32, q: vec3f) -> vec4f {
  let e = iv(base, I_HEAD);
  let ma = iv(base, I_HEAD + 2u);
  let mb = iv(base, I_HEAD + 3u);
  return smin_g(ellipsoid_g(q - e.xyz, iv(base, I_HEAD + 1u).xyz), round_cone_g(q, ma.xyz, mb.xyz, ma.w, mb.w), e.w);
}

// ---- the creature: `.blend(k)` over the parts in `mask`, in fold order -------------------------------

const F_BASE: u32 = 0u;
const F_BOUND: u32 = 1u;
const F_FULL: u32 = 2u;

fn field(base: u32, mask: u32, p: vec3f, fp: f32, mode: u32) -> f32 {
  let misc = iv(base, I_MISC);
  let k = misc.x;
  var d = BIG + select(0.0, 1.0, misc.x == SALT);
  var m = mask;
  loop {
    if (m == 0u) { break; }
    let b = firstTrailingBit(m);
    m &= m - 1u;
    var pd: f32;
    if (b == 0u) {
      let q = to_rest(base, I_TINV, p);
      pd = torso_base_d(base, q);
      if (mode == F_FULL) {
        pd += misc.y * fbm_d(q + iv(base, I_SEED).xyz, fp, misc);
      } else if (mode == F_BOUND) {
        pd -= misc.y * 1.875;
      }
    } else if (b == 5u) {
      pd = head_d(base, to_rest(base, I_HINV, p));
    } else {
      let s = cone_slot(b);
      let a = iv(base, I_CONES + 2u * s);
      let e = iv(base, I_CONES + 2u * s + 1u);
      pd = round_cone_d(p, a.xyz, e.xyz, a.w, e.w);
      if (b >= 24u) {
        let pl = iv(base, I_PLANES + b - 24u);
        pd = max(pd, -(dot(pl.xyz, p) + pl.w));
      }
    }
    d = smin_d(d, pd, k);
  }
  return d;
}

/** Distance, world-space gradient and channels (x: torso-ness, y: hoof-ness), for shading. */
fn field_g(base: u32, mask: u32, p: vec3f, fp: f32) -> Sample {
  let misc = iv(base, I_MISC);
  let k = misc.x;
  var s = Sample(BIG, vec3f(0.0, 1.0, 0.0), vec2f(0.0));
  var m = mask;
  loop {
    if (m == 0u) { break; }
    let b = firstTrailingBit(m);
    m &= m - 1u;
    var t: vec4f;
    var ch = vec2f(0.0);
    if (b == 0u) {
      let r = torso_g(base, to_rest(base, I_TINV, p), fp, misc, iv(base, I_SEED));
      t = vec4f(r.x, dir_world(base, I_TINV, r.yzw));
      ch = vec2f(1.0, 0.0);
    } else if (b == 5u) {
      let r = head_g(base, to_rest(base, I_HINV, p));
      t = vec4f(r.x, dir_world(base, I_HINV, r.yzw));
    } else {
      let sl = cone_slot(b);
      let a = iv(base, I_CONES + 2u * sl);
      let e = iv(base, I_CONES + 2u * sl + 1u);
      t = round_cone_g(p, a.xyz, e.xyz, a.w, e.w);
      if (b >= 24u) {
        let pl = iv(base, I_PLANES + b - 24u);
        let plane = -(dot(pl.xyz, p) + pl.w);
        t = select(t, vec4f(plane, -pl.xyz), plane > t.x);
        ch = vec2f(0.0, 1.0);
      }
    }
    s = smin_s(s, Sample(t.x, t.yzw, ch), k);
  }
  return s;
}

// ---- bounds: culling against planes (binning) and exact ray intervals (tracing) -----------------------

/** Sphere vs a set of 4 inward-facing planes (unit normals). */
fn sphere_in(s: vec4f, planes: array<vec4f, 4>) -> bool {
  var pl = planes;
  for (var i = 0u; i < 4u; i++) {
    if (dot(pl[i].xyz, s.xyz) + pl[i].w < -s.w) { return false; }
  }
  return true;
}

/** Capsule (a, b, radius r) vs 4 planes: outside only if both ends are beyond one plane by r. */
fn capsule_in(a: vec3f, b: vec3f, r: f32, planes: array<vec4f, 4>) -> bool {
  var pl = planes;
  for (var i = 0u; i < 4u; i++) {
    if (max(dot(pl[i].xyz, a), dot(pl[i].xyz, b)) + pl[i].w < -r) { return false; }
  }
  return true;
}

fn part_in(base: u32, b: u32, pl: array<vec4f, 4>) -> bool {
  if (b == 0u) { return sphere_in(iv(base, I_TSPH), pl) || sphere_in(iv(base, I_TSPH + 1u), pl); }
  if (b == 5u) {
    let ma = iv(base, I_MUZ);
    return sphere_in(iv(base, I_HSPH), pl) || capsule_in(ma.xyz, iv(base, I_MUZ + 1u).xyz, ma.w, pl);
  }
  let s = cone_slot(b);
  let a = iv(base, I_CONES + 2u * s);
  let e = iv(base, I_CONES + 2u * s + 1u);
  return capsule_in(a.xyz, e.xyz, max(a.w, e.w) + iv(base, I_MISC).x, pl);
}

const EMPTY: vec2f = vec2f(1e30, -1e30);

fn iv_union(a: vec2f, b: vec2f) -> vec2f { return vec2f(min(a.x, b.x), max(a.y, b.y)); }

fn ray_sphere(o: vec3f, d: vec3f, c: vec3f, r: f32) -> vec2f {
  let oc = o - c;
  let b = dot(oc, d);
  let h = b * b - (dot(oc, oc) - r * r);
  if (h < 0.0) { return EMPTY; }
  let s = sqrt(h);
  return vec2f(-b - s, -b + s);
}

/** Ray vs ellipsoid centred at the origin; `d` need not be unit (t is in its units). */
fn ray_ellipsoid(o: vec3f, d: vec3f, r: vec3f) -> vec2f {
  let ou = o / r;
  let du = d / r;
  let a = dot(du, du);
  let b = dot(ou, du);
  let h = b * b - a * (dot(ou, ou) - 1.0);
  if (h < 0.0) { return EMPTY; }
  let s = sqrt(h);
  return vec2f((-b - s) / a, (-b + s) / a);
}

/** Ray vs capsule: the union of its cylinder (clipped to the slab) and its two end spheres. */
fn ray_capsule(o: vec3f, d: vec3f, pa: vec3f, pb: vec3f, r: f32) -> vec2f {
  var res = iv_union(ray_sphere(o, d, pa, r), ray_sphere(o, d, pb, r));
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
      // Clip to the slab 0 ≤ dot(ba, p − pa) ≤ baba.
      if (abs(bard) > 1e-9) {
        let s0 = -baoa / bard;
        let s1 = (baba - baoa) / bard;
        t0 = max(t0, min(s0, s1));
        t1 = min(t1, max(s0, s1));
      } else if (baoa < 0.0 || baoa > baba) {
        t1 = t0 - 1.0;
      }
      if (t1 >= t0) { res = iv_union(res, vec2f(t0, t1)); }
    }
  }
  return res;
}

/** The ray's interval inside part b's inflated bound (EMPTY if it misses). */
fn part_interval(base: u32, b: u32, o: vec3f, d: vec3f) -> vec2f {
  if (b == 0u) {
    let ro = to_rest(base, I_TINV, o);
    let rd = dir_rest(base, I_TINV, d);
    return iv_union(ray_ellipsoid(ro - iv(base, I_TORSO).xyz, rd, iv(base, I_TRAD).xyz),
                    ray_ellipsoid(ro - iv(base, I_TORSO + 2u).xyz, rd, iv(base, I_TRAD + 1u).xyz));
  }
  if (b == 5u) {
    let ro = to_rest(base, I_HINV, o);
    let rd = dir_rest(base, I_HINV, d);
    let ma = iv(base, I_MUZ);
    return iv_union(ray_ellipsoid(ro - iv(base, I_HEAD).xyz, rd, iv(base, I_HRAD).xyz),
                    ray_capsule(o, d, ma.xyz, iv(base, I_MUZ + 1u).xyz, ma.w));
  }
  let s = cone_slot(b);
  let a = iv(base, I_CONES + 2u * s);
  let e = iv(base, I_CONES + 2u * s + 1u);
  return ray_capsule(o, d, a.xyz, e.xyz, max(a.w, e.w) + iv(base, I_MISC).x);
}
