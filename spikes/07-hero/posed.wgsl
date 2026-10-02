// The posed wolf: each part rigid in one bone's frame, or bent across a joint by a polar warp.
// Appended to common.wgsl + wolf.wgsl. The entry file declares `pose`.
//
// Parts 0–21, one per bone in the rigid decomposition (part i hangs on bone i):
//   0 croup, 1 loin, 2 chest, 3 neck, 4 head, 5–7 tail,
//   8–14 left scapula, upper arm, forearm, manus, thigh, gaskin, pes; 15–21 the same on the right.
// In warp mode part 9 (16) is the whole elbow region, part 13 (20) the whole hock region, part 3 the
// neck bent at its base; parts 10, 14, 17, 21 are switched off (no boxes). The fold is a smooth
// union in part order, each part with its own blend radius (the agent's radii at that junction).

// ---- bones -------------------------------------------------------------------------------------

fn to_rest(b: u32, p: vec3f) -> vec3f {
  let r0 = pose[P_BINV + 3u * b];
  let r1 = pose[P_BINV + 3u * b + 1u];
  let r2 = pose[P_BINV + 3u * b + 2u];
  return vec3f(dot(r0.xyz, p) + r0.w, dot(r1.xyz, p) + r1.w, dot(r2.xyz, p) + r2.w);
}

/** Rest → world rotation of bone b (rows of the forward transform). */
fn bone_rot(b: u32) -> mat3x3f {
  let r0 = pose[P_BFWD + 3u * b].xyz;
  let r1 = pose[P_BFWD + 3u * b + 1u].xyz;
  let r2 = pose[P_BFWD + 3u * b + 2u].xyz;
  return transpose(mat3x3f(r0, r1, r2));
}

// ---- the bend warp -------------------------------------------------------------------------------
// In the parent (A) bone's rest frame, around the hinge axis (x) through the joint J. A point's angle
// φ around the axis (measured from +z toward +y) is remapped to ψ = φ − θ·s(φ): s is 0 at the A
// direction (identity) and 1 at the posed B direction (rotated back by θ), with smoothstep
// transitions across the two gaps between them. Radius and x are unchanged, so the Jacobian is
// diag(1, 1, dψ/dφ) in cylindrical coordinates and the Lipschitz factor is max dψ/dφ, which wolf.js
// computes exactly per joint per frame: 1 + 1.5·|θ| / (width of the gap that closes).
//
// Joint block (8 vec4s at P_JNT + 8j):
//   [0] J.y, J.z, φ_A, P (posed B angle relative to A, in (0, 2π))
//   [1] θ, A wedge half-width, B wedge half-width, L
//   [2] refit domain min (A frame), part index
//   [3] refit domain max (A frame), A bone
//   [4] A-side box x axis, margin       [5] A-side box y axis
//   [6] B-side box x axis (A frame)     [7] B-side box y axis

struct Warped { q: vec3f, xr: f32, s: f32 }

fn warp(j: u32, q: vec3f) -> Warped {
  let a = pose[P_JNT + 8u * j];
  let b = pose[P_JNT + 8u * j + 1u];
  let v = vec2f(q.z - a.y, q.y - a.x);
  let phi = atan2(v.y, v.x);
  let u = fract((phi - a.z) / TAU) * TAU;
  var s: f32;
  if (u < a.w) { s = smoothstep(b.y, a.w - b.z, u); }
  else { s = 1.0 - smoothstep(a.w + b.z, TAU - b.y, u); }
  let dl = -b.x * s;                       // posed → rest; as an x-rotation it is also rest → posed
  let c = cos(dl);
  let sn = sin(dl);
  let v2 = vec2f(v.x * c - v.y * sn, v.x * sn + v.y * c);
  return Warped(vec3f(q.x, a.x + v2.y, a.y + v2.x), dl, s);
}

// ---- parts and the fold ----------------------------------------------------------------------------

fn base_fn(i: u32) -> u32 { return select(i, i - 7u, i >= 15u); }

/** Part i's value at world point p. `h` is its header [0]: eval bone, joint, L, k. */
fn part_at(i: u32, h: vec4f, p: vec3f) -> f32 {
  var q = to_rest(u32(h.x), p);
  let warped = h.y >= 0.0;
  if (warped) { q = warp(u32(h.y), q).q; }
  if (i >= 15u) { q.x = -q.x; }
  return rest_part(base_fn(i), q, warped);
}

fn part_k(i: u32) -> f32 { return pose[P_HDR + 2u * i].w; }
fn part_l(i: u32) -> f32 { return pose[P_HDR + 2u * i].z; }
fn part_nobb(i: u32) -> u32 { return u32(pose[P_HDR + 2u * i + 1u].x); }

/** The wolf over the parts in `mask`, folded in part order. */
fn field(mask: u32, p: vec3f) -> f32 {
  var d = BIG + select(0.0, 1.0, F.eye.w == SALT);
  var m = mask;
  loop {
    if (m == 0u) { break; }
    let i = firstTrailingBit(m);
    m &= m - 1u;
    let h = pose[P_HDR + 2u * i];
    d = smin(d, part_at(i, h, p), h.w);
  }
  return d;
}

/** For shading: distance, rest-space position and rest → world rotation, both blended across parts
 *  by the smooth union's weights (so texture coordinates and the coat's frame stay continuous across
 *  joints), and channels (tail, head). */
struct Sample { d: f32, q: vec3f, ch: vec2f, rot: mat3x3f }

fn xrot(a: f32) -> mat3x3f {
  let c = cos(a);
  let sn = sin(a);
  return mat3x3f(vec3f(1.0, 0.0, 0.0), vec3f(0.0, c, sn), vec3f(0.0, -sn, c));   // columns of Rx(a)
}

fn field_s(mask: u32, p: vec3f) -> Sample {
  var s = Sample(BIG, vec3f(0.0), vec2f(0.0), mat3x3f(vec3f(1.0, 0.0, 0.0), vec3f(0.0, 1.0, 0.0), vec3f(0.0, 0.0, 1.0)));
  var m = mask;
  loop {
    if (m == 0u) { break; }
    let i = firstTrailingBit(m);
    m &= m - 1u;
    let h = pose[P_HDR + 2u * i];
    let bone = u32(h.x);
    var q = to_rest(bone, p);
    var xr = 0.0;
    let warped = h.y >= 0.0;
    if (warped) {
      let w = warp(u32(h.y), q);
      q = w.q;
      xr = w.xr;
    }
    var qm = q;
    if (i >= 15u) { qm.x = -qm.x; }
    let f = base_fn(i);
    let v = rest_part(f, qm, warped);
    let ch = vec2f(select(0.0, 1.0, f >= 5u && f <= 7u), select(0.0, 1.0, f == 4u));
    let k = h.w;
    let wa = clamp(0.5 + 0.5 * (v - s.d) / k, 0.0, 1.0);   // weight of what's folded so far
    s.d = mix(v, s.d, wa) - k * wa * (1.0 - wa);
    s.q = mix(q, s.q, wa);
    s.ch = mix(ch, s.ch, wa);
    let r = bone_rot(bone) * xrot(xr);
    s.rot = mat3x3f(mix(r[0], s.rot[0], wa), mix(r[1], s.rot[1], wa), mix(r[2], s.rot[2], wa));
  }
  // Re-orthonormalize the blended rotation.
  let c0 = normalize(s.rot[0]);
  let c1 = normalize(s.rot[1] - c0 * dot(c0, s.rot[1]));
  s.rot = mat3x3f(c0, c1, cross(c0, c1));
  return s;
}

/** Normal from four evaluations (tetrahedral differences). */
fn field_n(mask: u32, p: vec3f, e: f32) -> vec3f {
  let k = vec2f(1.0, -1.0);
  return normalize(k.xyy * field(mask, p + k.xyy * e) + k.yyx * field(mask, p + k.yyx * e)
                 + k.yxy * field(mask, p + k.yxy * e) + k.xxx * field(mask, p + k.xxx * e));
}

// ---- bounds: oriented boxes, written per frame (rigid parts by wolf.js, warped parts by the refit) ---

const EMPTY: vec2f = vec2f(1e30, -1e30);

fn obb_interval(base: u32, o: vec3f, d: vec3f) -> vec2f {
  let c = pose[base].xyz;
  let ax = pose[base + 1u].xyz;
  let ay = pose[base + 2u].xyz;
  let az = pose[base + 3u].xyz;
  let oc = o - c;
  let ob = vec3f(dot(oc, ax), dot(oc, ay), dot(oc, az));
  let db = vec3f(dot(d, ax), dot(d, ay), dot(d, az));
  let inv = 1.0 / select(db, vec3f(1e-12), abs(db) < vec3f(1e-12));
  let t1 = (vec3f(-1.0) - ob) * inv;
  let t2 = (vec3f(1.0) - ob) * inv;
  let tn = min(t1, t2);
  let tf = max(t1, t2);
  let a = max(max(tn.x, tn.y), tn.z);
  let b = min(min(tf.x, tf.y), tf.z);
  return select(EMPTY, vec2f(a, b), b >= a);
}

fn part_interval(i: u32, o: vec3f, d: vec3f) -> vec2f {
  var r = EMPTY;
  let n = part_nobb(i);
  for (var k = 0u; k < n; k++) {
    let iv = obb_interval(P_OBB + 12u * i + 4u * k, o, d);
    r = vec2f(min(r.x, iv.x), max(r.y, iv.y));
  }
  return r;
}

/** Box vs 4 inward-facing planes (unit normals). */
fn obb_in_planes(base: u32, planes: array<vec4f, 4>) -> bool {
  var pl = planes;
  let c = pose[base].xyz;
  let ax = pose[base + 1u];
  let ay = pose[base + 2u];
  let az = pose[base + 3u];
  for (var k = 0u; k < 4u; k++) {
    let n = pl[k].xyz;
    let r = ax.w * ax.w * abs(dot(n, ax.xyz)) + ay.w * ay.w * abs(dot(n, ay.xyz)) + az.w * az.w * abs(dot(n, az.xyz));
    if (dot(n, c) + pl[k].w < -r) { return false; }
  }
  return true;
}

/** Box vs sphere. */
fn obb_near(base: u32, c: vec3f, r: f32) -> bool {
  let oc = c - pose[base].xyz;
  let ax = pose[base + 1u];
  let ay = pose[base + 2u];
  let az = pose[base + 3u];
  let e = vec3f(max(abs(dot(oc, ax.xyz)) - 1.0, 0.0) * ax.w, max(abs(dot(oc, ay.xyz)) - 1.0, 0.0) * ay.w,
                max(abs(dot(oc, az.xyz)) - 1.0, 0.0) * az.w);
  return dot(e, e) <= r * r;
}
