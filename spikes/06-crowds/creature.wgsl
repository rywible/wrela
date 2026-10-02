// Moving things as fields: every part is a round cone, rigid in world space after posing, and an
// instance is the smooth union of its parts. Needs `inst: array<vec4f>` declared by the pass.
//
// Spike 02's rule carries over: rigid parts keep the distance bound, and a part can be left out
// of a ray's mask when the ray never enters its bound inflated by the blend radius k.

struct Sample { d: f32, g: vec3f, ch: f32 }

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

fn smin_s(a: Sample, b: Sample, k: f32) -> Sample {
  let h = clamp(0.5 + 0.5 * (b.d - a.d) / k, 0.0, 1.0);
  return Sample(mix(b.d, a.d, h) - k * h * (1.0 - h), mix(b.g, a.g, h), mix(b.ch, a.ch, h));
}

fn hdr(base: u32) -> vec4f { return inst[base + I_HDR]; }
fn enabled_parts(base: u32) -> u32 { return u32(inst[base + I_HDR].z); }
fn part_a(base: u32, b: u32) -> vec4f { return inst[base + I_PARTS + 2u * b]; }
fn part_b(base: u32, b: u32) -> vec4f { return inst[base + I_PARTS + 2u * b + 1u]; }

/** The instance's field over the parts in `mask` (bits in fold order). */
fn cfield(base: u32, mask: u32, p: vec3f) -> f32 {
  let k = inst[base + I_HDR].x;
  var d = BIG;
  var m = mask;
  loop {
    if (m == 0u) { break; }
    let b = firstTrailingBit(m);
    m &= m - 1u;
    let a = part_a(base, b);
    let e = part_b(base, b);
    d = smin(d, round_cone_d(p, a.xyz, e.xyz, a.w, e.w), k);
  }
  return d;
}

/** Distance, gradient and the secondary-colour weight, for shading. */
fn cfield_g(base: u32, mask: u32, p: vec3f) -> Sample {
  let h = inst[base + I_HDR];
  let k = h.x;
  let second = u32(h.w);
  var s = Sample(BIG, vec3f(0.0, 1.0, 0.0), 0.0);
  var m = mask;
  loop {
    if (m == 0u) { break; }
    let b = firstTrailingBit(m);
    m &= m - 1u;
    let a = part_a(base, b);
    let e = part_b(base, b);
    let t = round_cone_g(p, a.xyz, e.xyz, a.w, e.w);
    s = smin_s(s, Sample(t.x, t.yzw, select(0.0, 1.0, ((second >> b) & 1u) == 1u)), k);
  }
  return s;
}

// ---- bounds: culling against planes (binning) and exact ray intervals (tracing) ----------------------

fn sphere_in(s: vec4f, pl: array<vec4f, 4>) -> bool {
  var q = pl;
  for (var i = 0u; i < 4u; i++) {
    if (dot(q[i].xyz, s.xyz) + q[i].w < -s.w) { return false; }
  }
  return true;
}

fn capsule_in(a: vec3f, b: vec3f, r: f32, pl: array<vec4f, 4>) -> bool {
  var q = pl;
  for (var i = 0u; i < 4u; i++) {
    if (max(dot(q[i].xyz, a), dot(q[i].xyz, b)) + q[i].w < -r) { return false; }
  }
  return true;
}

/** Part b's bound: the capsule around its round cone, inflated by the blend radius. */
fn part_in(base: u32, b: u32, k: f32, pl: array<vec4f, 4>) -> bool {
  let a = part_a(base, b);
  let e = part_b(base, b);
  return capsule_in(a.xyz, e.xyz, max(a.w, e.w) + k, pl);
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

/** Ray vs capsule: its cylinder (clipped to the slab) and its two end spheres (spike 02). */
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

fn part_interval(base: u32, b: u32, k: f32, o: vec3f, d: vec3f) -> vec2f {
  let a = part_a(base, b);
  let e = part_b(base, b);
  return ray_capsule(o, d, a.xyz, e.xyz, max(a.w, e.w) + k);
}

struct RayMask { m: u32, t0: f32, t1: f32 }

/** Per-ray part mask and the [t0, t1] it spans, from the candidate parts in `m_in`. */
fn ray_mask(base: u32, m_in: u32, o: vec3f, d: vec3f, limit: f32, inflate: f32) -> RayMask {
  let k = inst[base + I_HDR].x + inflate;
  var m = m_in;
  var pm = 0u;
  var t0 = INF;
  var t1 = 0.0;
  loop {
    if (m == 0u) { break; }
    let b = firstTrailingBit(m);
    m &= m - 1u;
    let r = part_interval(base, b, k, o, d);
    if (r.y > 0.0 && r.x <= r.y && r.x < limit) {
      pm |= 1u << b;
      t0 = min(t0, r.x);
      t1 = max(t1, r.y);
    }
  }
  return RayMask(pm, t0, t1);
}
