// Bounds by sampling, made conservative with the field's Lipschitz constant: a grid cell whose centre
// value is at least margin + L·(half-diagonal) holds no point below the margin, so the box around the
// remaining cells contains the part's whole margin-sublevel set. Appended to common + wolf + posed.
//
// - `bounds` (startup): each rigid part's box in rest space, at two margins (without and with fur).
// - `refit_*` (every frame, warp mode only): a bent joint region has no fixed box, so it is
//   resampled through the warp in its parent bone's frame. Cells go to the parent-side or child-side
//   box by the warp's blend s at the cell centre, and the two boxes are written into the pose buffer.

@group(0) @binding(1) var<storage, read_write> pose: array<vec4f>;
@group(0) @binding(2) var<storage, read> jobs: array<vec4f>;
@group(0) @binding(3) var<storage, read_write> acc: array<atomic<u32>>;
@group(0) @binding(4) var<uniform> job_index: vec4u;   // startup: which job this dispatch samples

const LIP: f32 = 1.25;    // assumed Lipschitz constant of the unwarped rest parts (sampled cells only)
const BG: u32 = 80u;      // startup grid cells per axis
const RG: u32 = 32u;      // refit grid cells per axis
const ACC_STRIDE: u32 = 16u;

fn ord(f: f32) -> u32 {
  let b = bitcast<u32>(f);
  return select(~b, b | 0x80000000u, f >= 0.0);
}

fn project(slot: u32, c: vec3f, half: vec3f, ax: vec3f, ay: vec3f, az: vec3f) {
  let cx = dot(c, ax);
  let cy = dot(c, ay);
  let cz = dot(c, az);
  let rx = dot(abs(ax), half);
  let ry = dot(abs(ay), half);
  let rz = dot(abs(az), half);
  atomicMin(&acc[slot + 0u], ord(cx - rx));
  atomicMin(&acc[slot + 1u], ord(cy - ry));
  atomicMin(&acc[slot + 2u], ord(cz - rz));
  atomicMax(&acc[slot + 3u], ord(cx + rx));
  atomicMax(&acc[slot + 4u], ord(cy + ry));
  atomicMax(&acc[slot + 5u], ord(cz + rz));
}

// Job (4 vec4s): [0] domain min, base fn; [1] domain max, warped; [2] x axis, margin a; [3] y axis, margin b.
// Accumulators per job: margin a (6), margin b (6), boundary flag (1).
@compute @workgroup_size(4, 4, 4)
fn bounds(@builtin(global_invocation_id) gid: vec3u) {
  let job = job_index.x;
  let cell = gid;
  let j0 = jobs[4u * job];
  let j1 = jobs[4u * job + 1u];
  let j2 = jobs[4u * job + 2u];
  let j3 = jobs[4u * job + 3u];
  let cs = (j1.xyz - j0.xyz) / f32(BG);
  let c = j0.xyz + (vec3f(cell) + 0.5) * cs;
  let v = rest_part(u32(j0.w), c, j1.w > 0.5) + select(0.0, 1.0, F.eye.w == SALT);
  let hd = 0.5 * length(cs);
  let ax = j2.xyz;
  let ay = j3.xyz;
  let az = cross(ax, ay);
  let edge = any(cell == vec3u(0u)) || any(cell == vec3u(BG - 1u));
  let base = job * ACC_STRIDE;
  if (v < j2.w + LIP * hd) {
    project(base, c, 0.5 * cs, ax, ay, az);
    if (edge) { atomicOr(&acc[base + 12u], 1u); }
  }
  if (v < j3.w + LIP * hd) {
    project(base + 6u, c, 0.5 * cs, ax, ay, az);
    if (edge) { atomicOr(&acc[base + 12u], 2u); }
  }
}

// ---- per-frame refit of the warped joint regions -----------------------------------------------------

@compute @workgroup_size(64)
fn refit_clear(@builtin(global_invocation_id) gid: vec3u) {
  let i = gid.x;
  if (i < NJ * ACC_STRIDE) {
    let k = i % ACC_STRIDE;
    atomicStore(&acc[i], select(0u, 0xFFFFFFFFu, k < 3u || (k >= 6u && k < 9u)));
  }
}

@compute @workgroup_size(64)
fn refit_sample(@builtin(global_invocation_id) gid: vec3u) {
  let j = gid.y;
  let jb = P_JNT + 8u * j;
  let b2 = pose[jb + 2u];
  if (b2.w < 0.0 || gid.x >= RG * RG * RG) { return; }
  let b3 = pose[jb + 3u];
  let cell = vec3u(gid.x % RG, (gid.x / RG) % RG, gid.x / (RG * RG));
  let cs = (b3.xyz - b2.xyz) / f32(RG);
  let q = b2.xyz + (vec3f(cell) + 0.5) * cs;
  let w = warp(j, q);
  let part = u32(b2.w);
  var qm = w.q;
  if (part >= 15u) { qm.x = -qm.x; }
  let v = rest_part(base_fn(part), qm, true);
  let L = pose[jb + 1u].w * LIP;
  let a4 = pose[jb + 4u];
  let margin = a4.w + F.fur.w;
  if (v >= margin + L * 0.5 * length(cs)) { return; }
  let side = select(1u, 0u, w.s < 0.5);
  let ax = pose[jb + 4u + 2u * side].xyz;
  let ay = pose[jb + 5u + 2u * side].xyz;
  let base = j * ACC_STRIDE + 6u * side;
  project(base, q, 0.5 * cs, ax, ay, cross(ax, ay));
  if (any(cell == vec3u(0u)) || any(cell == vec3u(RG - 1u))) { atomicOr(&acc[j * ACC_STRIDE + 12u], 1u); }
}

fn unord(u: u32) -> f32 {
  return select(bitcast<f32>(~u), bitcast<f32>(u & 0x7fffffffu), (u & 0x80000000u) != 0u);
}

@compute @workgroup_size(16)
fn refit_finalize(@builtin(global_invocation_id) gid: vec3u) {
  let j = gid.x / 2u;
  let side = gid.x % 2u;
  if (j >= NJ) { return; }
  let jb = P_JNT + 8u * j;
  let b2 = pose[jb + 2u];
  if (b2.w < 0.0) { return; }
  let part = u32(b2.w);
  let bone = u32(pose[jb + 3u].w);
  let ax = pose[jb + 4u + 2u * side].xyz;
  let ay = pose[jb + 5u + 2u * side].xyz;
  let az = cross(ax, ay);
  let base = j * ACC_STRIDE + 6u * side;
  let lo = vec3f(unord(atomicLoad(&acc[base])), unord(atomicLoad(&acc[base + 1u])), unord(atomicLoad(&acc[base + 2u])));
  let hi = vec3f(unord(atomicLoad(&acc[base + 3u])), unord(atomicLoad(&acc[base + 4u])), unord(atomicLoad(&acc[base + 5u])));
  let o = P_OBB + 12u * part + 4u * side;
  if (any(lo > hi)) {                        // nothing on this side: a box nobody can hit
    pose[o] = vec4f(0.0, -1000.0, 0.0, 0.0);
    pose[o + 1u] = vec4f(1000.0, 0.0, 0.0, 0.001);
    pose[o + 2u] = vec4f(0.0, 1000.0, 0.0, 0.001);
    pose[o + 3u] = vec4f(0.0, 0.0, 1000.0, 0.001);
    return;
  }
  let mid = 0.5 * (lo + hi);
  let h = max(0.5 * (hi - lo), vec3f(1e-3));
  let cr = ax * mid.x + ay * mid.y + az * mid.z;            // centre, parent bone's rest frame
  let R = bone_rot(bone);
  let r0 = pose[P_BFWD + 3u * bone];
  let r1 = pose[P_BFWD + 3u * bone + 1u];
  let r2 = pose[P_BFWD + 3u * bone + 2u];
  let cw = vec3f(dot(r0.xyz, cr) + r0.w, dot(r1.xyz, cr) + r1.w, dot(r2.xyz, cr) + r2.w);
  pose[o] = vec4f(cw, 0.0);
  pose[o + 1u] = vec4f(R * ax / h.x, h.x);
  pose[o + 2u] = vec4f(R * ay / h.y, h.y);
  pose[o + 3u] = vec4f(R * az / h.z, h.z);
}
