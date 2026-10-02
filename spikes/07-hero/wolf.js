// The wolf's skeleton, its walk cycle, and the per-frame pose buffer (layout in common.wgsl).
//
// Joints come from experiments/agent-authoring/wolf/creature.wgsl. Every bone rotates about x (the
// sagittal plane) except the tail, which also sways. Legs use two-bone IK: a foot path per leg, a
// distal-segment angle per phase (carpus and hock flexion), then the two upper segments solve for
// the remaining joint. This is presentation code, not compiler output.

export const J = {
  PELVIS: [0, 0.632, -0.430], LUMBAR: [0, 0.655, -0.180], CHEST: [0, 0.615, 0.095],
  NECK: [0, 0.680, 0.310], HEAD: [0, 0.912, 0.592],
  TAIL0: [0, 0.668, -0.570], TAIL1: [0, 0.585, -0.660], TAIL2: [0, 0.450, -0.715], TAIL3: [0, 0.270, -0.690],
  SCAPULA: [0.050, 0.750, 0.185], SHOULDER: [0.065, 0.565, 0.335], ELBOW: [0.078, 0.415, 0.245],
  CARPUS: [0.072, 0.128, 0.270], FPAW: [0.070, 0.000, 0.300],
  HIP: [0.070, 0.600, -0.420], STIFLE: [0.085, 0.410, -0.320], HOCK: [0.078, 0.210, -0.495], HPAW: [0.075, 0.000, -0.445],
};
const NECK_END = [0, 0.882, 0.522];   // J_HEAD + (0, −0.03, −0.07), the neck cone's far end

// Pose buffer layout (vec4s), must match common.wgsl.
export const P = { BINV: 0, BFWD: 66, HDR: 132, OBB: 176, JNT: 440, SIZE: 480 };
export const NB = 22, NJ = 5;

// Parts: base function (wolf.wgsl), fold blend radius k, and the sampling domain for its bounds
// (rest space, left side) with the box axes (segment-aligned for limbs).
const seg = (a, b) => { const v = [0, b[1] - a[1], b[2] - a[2]]; const l = Math.hypot(v[1], v[2]); return [0, v[1] / l, v[2] / l]; };
const Y = [0, 1, 0];
export const PARTS = [
  { name: 'croup', k: 0.08, lo: [-0.32, 0.30, -0.80], hi: [0.32, 0.96, -0.08], y: Y },
  { name: 'loin', k: 0.08, lo: [-0.28, 0.36, -0.58], hi: [0.28, 0.95, 0.22], y: Y },
  { name: 'chest', k: 0.08, lo: [-0.34, 0.26, -0.36], hi: [0.34, 1.02, 0.62], y: Y },
  { name: 'neck', k: 0.08, lo: [-0.34, 0.40, 0.00], hi: [0.34, 1.12, 0.76], y: seg(J.NECK, NECK_END) },
  { name: 'head', k: 0.05, lo: [-0.24, 0.64, 0.30], hi: [0.24, 1.18, 1.02], y: Y },
  { name: 'tail0', k: 0.04, lo: [-0.20, 0.38, -0.86], hi: [0.20, 0.88, -0.36], y: seg(J.TAIL0, J.TAIL1) },
  { name: 'tail1', k: 0.02, lo: [-0.20, 0.26, -0.92], hi: [0.20, 0.80, -0.44], y: seg(J.TAIL1, J.TAIL2) },
  { name: 'tail2', k: 0.03, lo: [-0.18, 0.10, -0.92], hi: [0.18, 0.64, -0.48], y: seg(J.TAIL2, J.TAIL3) },
  { name: 'scapula', k: 0.05, lo: [-0.14, 0.38, -0.04], hi: [0.30, 0.98, 0.58], y: seg(J.SCAPULA, J.SHOULDER) },
  { name: 'upperarm', k: 0.05, lo: [-0.14, 0.20, 0.00], hi: [0.30, 0.80, 0.58], y: seg(J.SHOULDER, J.ELBOW) },
  { name: 'forearm', k: 0.03, lo: [-0.10, -0.04, 0.04], hi: [0.26, 0.60, 0.48], y: seg(J.ELBOW, J.CARPUS) },
  { name: 'manus', k: 0.015, lo: [-0.08, -0.10, 0.08], hi: [0.24, 0.30, 0.52], y: seg(J.CARPUS, [0.07, 0.04, 0.292]) },
  { name: 'thigh', k: 0.07, lo: [-0.14, 0.10, -0.84], hi: [0.36, 0.90, 0.02], y: seg(J.HIP, J.STIFLE) },
  { name: 'gaskin', k: 0.05, lo: [-0.12, 0.02, -0.76], hi: [0.30, 0.62, -0.10], y: seg(J.STIFLE, J.HOCK) },
  { name: 'pes', k: 0.02, lo: [-0.10, -0.10, -0.70], hi: [0.26, 0.40, -0.20], y: seg(J.HOCK, [0.075, 0.04, -0.455]) },
];
export const MARGIN_SLACK = 0.02;   // added to each part's fold k for the box margin (README)

// Joint-region jobs for the warp's refit domain: the combined rest function at a larger margin.
export const JOINT_JOBS = [
  { name: 'neck', fn: 3, warped: 0, lo: [-0.40, 0.30, -0.10], hi: [0.40, 1.22, 0.86], y: seg(J.NECK, NECK_END) },
  { name: 'elbow', fn: 9, warped: 1, lo: [-0.18, -0.10, -0.06], hi: [0.34, 0.86, 0.64], y: seg(J.SHOULDER, J.ELBOW) },
  { name: 'hock', fn: 13, warped: 1, lo: [-0.16, -0.14, -0.82], hi: [0.34, 0.68, -0.06], y: seg(J.STIFLE, J.HOCK) },
];

// Bones (part i hangs on bone i). Right-side limbs are 15–21 (mirrors of 8–14).
export const BONE_NAMES = ['pelvis', 'lumbar', 'chest', 'neck', 'head', 'tail0', 'tail1', 'tail2',
  'scapula_L', 'upperarm_L', 'forearm_L', 'manus_L', 'thigh_L', 'gaskin_L', 'pes_L',
  'scapula_R', 'upperarm_R', 'forearm_R', 'manus_R', 'thigh_R', 'gaskin_R', 'pes_R'];

// Joints for the bend warp: [part, A bone, B part (switched off in warp mode), hinge, A dir, B dir, wedges].
const phiOf = (from, to) => Math.atan2(to[1] - from[1], to[2] - from[2]);
// fn: the joint part's base function; aAx/bAx: base functions whose segment axes orient the two side
// boxes of the refit; wa/wb: wedge half-widths (rad) kept exactly rigid around each segment.
export const JOINTS = [
  { name: 'neck', part: 3, fn: 3, aBone: 2, off: -1, aAx: 2, bAx: 3, hinge: J.NECK, phiA: phiOf(J.NECK, J.CHEST), phiB: phiOf(J.NECK, NECK_END), wa: 0.0, wb: 0.26, job: 0 },
  { name: 'elbow_L', part: 9, fn: 9, aBone: 9, off: 10, aAx: 9, bAx: 10, hinge: J.ELBOW, phiA: phiOf(J.ELBOW, J.SHOULDER), phiB: phiOf(J.ELBOW, J.CARPUS), wa: 0.10, wb: 0.10, job: 1 },
  { name: 'hock_L', part: 13, fn: 13, aBone: 13, off: 14, aAx: 13, bAx: 14, hinge: J.HOCK, phiA: phiOf(J.HOCK, J.STIFLE), phiB: phiOf(J.HOCK, J.HPAW), wa: 0.10, wb: 0.10, job: 2 },
  { name: 'elbow_R', part: 16, fn: 9, aBone: 16, off: 17, aAx: 9, bAx: 10, hinge: J.ELBOW, phiA: phiOf(J.ELBOW, J.SHOULDER), phiB: phiOf(J.ELBOW, J.CARPUS), wa: 0.10, wb: 0.10, job: 1, right: true },
  { name: 'hock_R', part: 20, fn: 13, aBone: 20, off: 21, aAx: 13, bAx: 14, hinge: J.HOCK, phiA: phiOf(J.HOCK, J.STIFLE), phiB: phiOf(J.HOCK, J.HPAW), wa: 0.10, wb: 0.10, job: 2, right: true },
];
const jointAlpha = (info, j) => info.alpha[j.name] ?? 0;

// ---- small rigid-transform maths (3×4 row-major: [r00 r01 r02 t0, r10 r11 r12 t1, r20 r21 r22 t2]) ----

const I3 = () => [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0];
const rx = a => { const c = Math.cos(a), s = Math.sin(a); return [1, 0, 0, 0, 0, c, -s, 0, 0, s, c, 0]; };
const ry = a => { const c = Math.cos(a), s = Math.sin(a); return [c, 0, s, 0, 0, 1, 0, 0, -s, 0, c, 0]; };
export function mul(a, b) {
  const o = new Array(12);
  for (let r = 0; r < 3; r++) {
    for (let c = 0; c < 3; c++) o[r * 4 + c] = a[r * 4] * b[c] + a[r * 4 + 1] * b[4 + c] + a[r * 4 + 2] * b[8 + c];
    o[r * 4 + 3] = a[r * 4] * b[3] + a[r * 4 + 1] * b[7] + a[r * 4 + 2] * b[11] + a[r * 4 + 3];
  }
  return o;
}
export const apply = (m, p) => [0, 1, 2].map(r => m[r * 4] * p[0] + m[r * 4 + 1] * p[1] + m[r * 4 + 2] * p[2] + m[r * 4 + 3]);
export const rotate = (m, v) => [0, 1, 2].map(r => m[r * 4] * v[0] + m[r * 4 + 1] * v[1] + m[r * 4 + 2] * v[2]);
export function inverse(m) {   // rigid
  const o = new Array(12);
  for (let r = 0; r < 3; r++) for (let c = 0; c < 3; c++) o[r * 4 + c] = m[c * 4 + r];
  for (let r = 0; r < 3; r++) o[r * 4 + 3] = -(o[r * 4] * m[3] + o[r * 4 + 1] * m[7] + o[r * 4 + 2] * m[11]);
  return o;
}
/** Rotation R about a pivot that moves from `rest` to `posed`: p ↦ posed + R (p − rest). */
function pivot(rest, posed, R) {
  const t = rotate(R, rest);
  const m = R.slice();
  m[3] = posed[0] - t[0]; m[7] = posed[1] - t[1]; m[11] = posed[2] - t[2];
  return m;
}
const translate = t => [1, 0, 0, t[0], 0, 1, 0, t[1], 0, 0, 1, t[2]];

// ---- the walk ----------------------------------------------------------------------------------

export const WALK = { period: 0.9, duty: 0.6, speed: 0.8, liftF: 0.075, liftH: 0.065 };
const ss = x => { const t = Math.min(Math.max(x, 0), 1); return t * t * (3 - 2 * t); };
const lerp = (a, b, t) => a + (b - a) * t;
// φ (angle from +z toward +y) of a (y, z) vector; Rx(α) decreases φ by α.
const ang = (y, z) => Math.atan2(y, z);

function footPath(phi, sweep, lift) {
  const { duty } = WALK;
  if (phi < duty) { const u = phi / duty; return { dz: sweep * (0.5 - u), dy: 0, u, stance: true }; }
  const u = (phi - duty) / (1 - duty);
  const s = u - Math.sin(2 * Math.PI * u) / (2 * Math.PI);
  return { dz: sweep * (-0.5 + s), dy: lift * Math.pow(Math.sin(Math.PI * u), 1.2), u, stance: false };
}

function manusAngle(f) {   // carpus: heel lift late in stance, folds back in swing
  if (f.stance) return 0.32 * ss((f.u - 0.65) / 0.35);
  const u = f.u;
  if (u < 0.35) return lerp(0.32, 1.45, ss(u / 0.35));
  if (u < 0.85) return lerp(1.45, -0.08, ss((u - 0.35) / 0.5));
  return lerp(-0.08, 0, ss((u - 0.85) / 0.15));
}

function pesAngle(f) {     // hock: tilts back through stance, flexes forward in swing
  if (f.stance) return lerp(-0.10, 0.32, ss((f.u - 0.2) / 0.8));
  const u = f.u;
  if (u < 0.45) return lerp(0.32, -0.55, ss(u / 0.45));
  return lerp(-0.55, -0.10, ss((u - 0.45) / 0.55));
}

/** Two-bone IK in the (y, z) plane: the middle joint between a (fixed) and c (target). `front`
 *  picks the solution whose middle joint is further forward (+z). */
function ik2(a, c, l1, l2, front) {
  let dy = c.y - a.y, dz = c.z - a.z;
  let D = Math.hypot(dy, dz);
  const Dc = Math.min(Math.max(D, Math.abs(l1 - l2) + 1e-4), l1 + l2 - 1e-4);
  const base = ang(dy, dz);
  const k = Math.acos(Math.min(Math.max((l1 * l1 + Dc * Dc - l2 * l2) / (2 * l1 * Dc), -1), 1));
  const m1 = { y: a.y + l1 * Math.sin(base + k), z: a.z + l1 * Math.cos(base + k) };
  const m2 = { y: a.y + l1 * Math.sin(base - k), z: a.z + l1 * Math.cos(base - k) };
  const pick = front ? (m1.z > m2.z ? m1 : m2) : (m1.z < m2.z ? m1 : m2);
  return { m: pick, reach: D / (l1 + l2) };
}

const yz = p => ({ y: p[1], z: p[2] });
const len2 = (a, b) => Math.hypot(b[1] - a[1], b[2] - a[2]);
const segAng = (a, b) => ang(b[1] - a[1], b[2] - a[2]);
const rotYZ = (v, a) => ({ y: v.y * Math.cos(a) - v.z * Math.sin(a), z: v.y * Math.sin(a) + v.z * Math.cos(a) });  // Rx(a) on (y, z)

/** Bone transforms (rest → world) at time t, plus the joint angles the warp needs. */
export function poseWolf(t, opts = {}) {
  const { period, speed } = WALK;
  const sweep = speed * WALK.duty * period;
  const phase = t / period;
  const bob = 0.008 * Math.cos(4 * Math.PI * phase);
  const root = opts.root || [0, 0, 0];
  const R = translate([root[0], root[1] + bob, root[2] + speed * t]);
  const local = new Array(NB);
  const info = { reach: [], alpha: {} };
  local[0] = I3(); local[1] = I3(); local[2] = I3();

  // Neck lowered for walking, bobbing twice per cycle; the head counter-rotates.
  const an = 0.30 + 0.045 * Math.sin(4 * Math.PI * phase + 0.6) + (opts.neck || 0);
  const ah = -0.20 - 0.03 * Math.sin(4 * Math.PI * phase + 0.6) - 0.5 * (opts.neck || 0);
  local[3] = pivot(J.NECK, J.NECK, rx(an));
  local[4] = pivot(J.HEAD, apply(local[3], J.HEAD), rx(an + ah));
  info.alpha.neck = an;

  // Tail: hangs a little lower, sways side to side with a lag down the chain.
  const tj = [J.TAIL0, J.TAIL1, J.TAIL2];
  let absR = I3();
  for (let i = 0; i < 3; i++) {
    const yaw = (0.09 + 0.03 * i) * Math.sin(2 * Math.PI * phase - 0.6 * i);
    const pitch = i === 0 ? 0.10 : 0.04;
    const posedJ = i === 0 ? tj[0] : apply(local[4 + i], tj[i]);
    absR = mul(absR, mul(ry(yaw), rx(pitch)));
    local[5 + i] = pivot(tj[i], posedJ, absR);
  }

  // Legs. Offsets: lateral-sequence walk (LH, LF, RH, RF).
  const legs = [
    { side: 0, fore: true, off: 0.25 }, { side: 0, fore: false, off: 0.0 },
    { side: 1, fore: true, off: 0.75 }, { side: 1, fore: false, off: 0.5 },
  ];
  for (const leg of legs) {
    const ph = ((phase + leg.off) % 1 + 1) % 1;
    const base = leg.side ? 15 : 8;
    if (leg.fore) {
      const f = footPath(ph, sweep, WALK.liftF);
      const paw = { y: J.FPAW[1] + f.dy - bob, z: J.FPAW[2] + f.dz };
      const am = manusAngle(f);
      const asc = -0.20 * (f.dz / (sweep / 2));
      const v = rotYZ({ y: J.FPAW[1] - J.CARPUS[1], z: J.FPAW[2] - J.CARPUS[2] }, am);
      const C = { y: paw.y - v.y, z: paw.z - v.z };
      const sv = rotYZ({ y: J.SHOULDER[1] - J.SCAPULA[1], z: J.SHOULDER[2] - J.SCAPULA[2] }, asc);
      const S = { y: J.SCAPULA[1] + sv.y, z: J.SCAPULA[2] + sv.z };
      const { m: E, reach } = ik2(S, C, len2(J.SHOULDER, J.ELBOW), len2(J.ELBOW, J.CARPUS), false);
      const au = segAng(J.SHOULDER, J.ELBOW) - ang(E.y - S.y, E.z - S.z);
      const af = segAng(J.ELBOW, J.CARPUS) - ang(C.y - E.y, C.z - E.z);
      const x = j => j[0];
      local[base] = pivot(J.SCAPULA, J.SCAPULA, rx(asc));
      local[base + 1] = pivot(J.SHOULDER, [x(J.SHOULDER), S.y, S.z], rx(au));
      local[base + 2] = pivot(J.ELBOW, [x(J.ELBOW), E.y, E.z], rx(af));
      local[base + 3] = pivot(J.CARPUS, [x(J.CARPUS), C.y, C.z], rx(am));
      info.reach.push(reach);
      info.alpha[`elbow_${leg.side ? 'R' : 'L'}`] = af - au;
      info.alpha[`carpus_${leg.side ? 'R' : 'L'}`] = am - af;
    } else {
      const f = footPath(ph, sweep, WALK.liftH);
      const paw = { y: J.HPAW[1] + f.dy - bob, z: J.HPAW[2] + f.dz };
      const ap = pesAngle(f);
      const v = rotYZ({ y: J.HPAW[1] - J.HOCK[1], z: J.HPAW[2] - J.HOCK[2] }, ap);
      const Hk = { y: paw.y - v.y, z: paw.z - v.z };
      const H = yz(J.HIP);
      const { m: St, reach } = ik2(H, Hk, len2(J.HIP, J.STIFLE), len2(J.STIFLE, J.HOCK), true);
      const at = segAng(J.HIP, J.STIFLE) - ang(St.y - H.y, St.z - H.z);
      const ag = segAng(J.STIFLE, J.HOCK) - ang(Hk.y - St.y, Hk.z - St.z);
      local[base + 4] = pivot(J.HIP, J.HIP, rx(at));
      local[base + 5] = pivot(J.STIFLE, [J.STIFLE[0], St.y, St.z], rx(ag));
      local[base + 6] = pivot(J.HOCK, [J.HOCK[0], Hk.y, Hk.z], rx(ap));
      info.reach.push(reach);
      info.alpha[`hock_${leg.side ? 'R' : 'L'}`] = ap - ag;
      info.alpha[`stifle_${leg.side ? 'R' : 'L'}`] = ag - at;
    }
  }
  // Right-side legs: the same transforms mirrored in x (their rest joints are mirrored).
  for (let b = 15; b < 22; b++) local[b] = mirrorX(local[b]);
  const world = local.map(m => mul(R, m));
  return { world, info, root: apply(R, [0, 0, 0]) };
}

/** Conjugate by the x mirror: S M S with S = diag(−1, 1, 1). */
function mirrorX(m) {
  const s = [-1, 1, 1];
  const o = new Array(12);
  for (let r = 0; r < 3; r++) {
    for (let c = 0; c < 3; c++) o[r * 4 + c] = s[r] * m[r * 4 + c] * s[c];
    o[r * 4 + 3] = s[r] * m[r * 4 + 3];
  }
  return o;
}

// ---- the pose buffer --------------------------------------------------------------------------------

/** Box (rest, left side) as {c, ax:[x,y,z axes], h:[hx,hy,hz]}; mirrored for the right side. */
function mirrorBox(b) {
  return { c: [-b.c[0], b.c[1], b.c[2]], ax: b.ax.map(a => [-a[0], a[1], a[2]]), h: b.h };
}

function putBox(out, base, c, ax, h) {
  out.set([...c, 0], base * 4);
  for (let k = 0; k < 3; k++) out.set([ax[k][0] / h[k], ax[k][1] / h[k], ax[k][2] / h[k], h[k]], (base + 1 + k) * 4);
}

const wrap = a => ((a % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);

/** Bend-warp parameters for one joint at relative angle Δα (child minus parent, as an x-rotation). */
export function warpParams(j, dAlpha) {
  const theta = -dAlpha;                      // in φ (angle from +z toward +y)
  const P = wrap(j.phiB + theta - j.phiA);
  const w1 = P - j.wa - j.wb, w2 = 2 * Math.PI - P - j.wa - j.wb;
  const L1 = theta < 0 ? 1 + 1.5 * -theta / w1 : 1;
  const L2 = theta > 0 ? 1 + 1.5 * theta / w2 : 1;
  const fold = Math.max(theta > 0 ? 1.5 * theta / w1 : 0, theta < 0 ? 1.5 * -theta / w2 : 0);   // < 1 or the warp folds
  return { theta, P, L: Math.max(L1, L2, 1), fold, w1, w2 };
}

/**
 * Writes the pose buffer for one frame. `rest` holds the startup boxes:
 *   rest.parts[f] = { m0: box at margin, m1: box at margin + fur }, rest.joints[job] = box (big margin).
 * mode: 'rigid' | 'warp'. fur: whether boxes include the fur shell.
 */
export function writePose(out, pose, rest, mode, fur) {
  out.fill(0);
  const { world } = pose;
  for (let b = 0; b < NB; b++) {
    const inv = inverse(world[b]);
    out.set(inv, (P.BINV + 3 * b) * 4);
    out.set(world[b], (P.BFWD + 3 * b) * 4);
  }
  const warp = mode === 'warp';
  const jointOf = {};
  if (warp) JOINTS.forEach((j, i) => { jointOf[j.part] = i; });
  const off = new Set(warp ? JOINTS.filter(j => j.off >= 0).map(j => j.off) : []);
  const Ls = [];
  for (let i = 0; i < 22; i++) {
    const f = i >= 15 ? i - 7 : i;
    const def = PARTS[f];
    const ji = jointOf[i];
    const j = ji !== undefined ? JOINTS[ji] : null;
    let L = 1, evalBone = i, nobb = 1;
    if (off.has(i)) nobb = 0;
    if (j) {
      const wp = warpParams(j, jointAlpha(pose.info, j));
      L = wp.L;
      evalBone = j.aBone;
      nobb = 2;
      Ls.push(wp.L);
    }
    const margin = def.k + MARGIN_SLACK;
    out.set([evalBone, j ? ji : -1, L, def.k], (P.HDR + 2 * i) * 4);
    out.set([nobb, margin, 0, 0], (P.HDR + 2 * i + 1) * 4);
    if (nobb === 1) {
      let box = fur ? rest.parts[f].m1 : rest.parts[f].m0;
      if (i >= 15) box = mirrorBox(box);
      const M = world[i];
      putBox(out, P.OBB + 12 * i, apply(M, box.c), box.ax.map(a => rotate(M, a)), box.h);
    }
  }
  // Joint blocks (the warp and its refit inputs). Rigid mode leaves them off (part index −1).
  for (let ji = 0; ji < NJ; ji++) {
    const base = (P.JNT + 8 * ji) * 4;
    if (!warp) { out.set([0, 0, 0, -1], base + 8); continue; }
    const j = JOINTS[ji];
    const wp = warpParams(j, jointAlpha(pose.info, j));
    if (wp.fold >= 1) console.error(`warp ${j.name} folds: θ ${wp.theta.toFixed(3)}`);
    out.set([j.hinge[1], j.hinge[2], j.phiA, wp.P], base);
    out.set([wp.theta, j.wa, j.wb, wp.L], base + 4);
    // Refit domain: the joint region's rest box swept about the hinge from 0 to θ.
    let box = rest.joints[j.job];
    if (j.right) box = mirrorBox(box);
    const corners = [];
    for (let k = 0; k < 8; k++) {
      const s = [k & 1 ? 1 : -1, k & 2 ? 1 : -1, k & 4 ? 1 : -1];
      corners.push([0, 1, 2].map(a => box.c[a] + s[0] * box.ax[0][a] * box.h[0] + s[1] * box.ax[1][a] * box.h[1] + s[2] * box.ax[2][a] * box.h[2]));
    }
    const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    let rmax = 0;
    for (let st = 0; st <= 4; st++) {
      const a = wp.theta * st / 4;
      for (const p of corners) {
        const z = p[2] - j.hinge[2], y = p[1] - j.hinge[1];
        rmax = Math.max(rmax, Math.hypot(y, z));
        const q = [p[0], j.hinge[1] + z * Math.sin(a) + y * Math.cos(a), j.hinge[2] + z * Math.cos(a) - y * Math.sin(a)];
        for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], q[k]); hi[k] = Math.max(hi[k], q[k]); }
      }
    }
    const pad = rmax * (1 - Math.cos(wp.theta / 8)) + 0.01;
    out.set([lo[0] - pad, lo[1] - pad, lo[2] - pad, j.part], base + 8);
    out.set([hi[0] + pad, hi[1] + pad, hi[2] + pad, j.aBone], base + 12);
    // Side boxes' axes: A along the parent segment, B along the child segment rotated by θ (A frame).
    const aY = PARTS[j.aAx].y;      // segment axes have no x component, so mirroring leaves them alone
    const bYrest = PARTS[j.bAx].y;
    const th = wp.theta;
    const bY = [0, bYrest[2] * Math.sin(th) + bYrest[1] * Math.cos(th), bYrest[2] * Math.cos(th) - bYrest[1] * Math.sin(th)];
    out.set([1, 0, 0, PARTS[j.fn].k + MARGIN_SLACK], base + 16);
    out.set([...aY, 0], base + 20);
    out.set([1, 0, 0, 0], base + 24);
    out.set([...bY, 0], base + 28);
  }
  return { L: Ls };
}

/** Palette for the raster path: rest → world as column-major mat4s. */
export function writePalette(out, pose) {
  for (let b = 0; b < NB; b++) {
    const m = pose.world[b];
    out.set([m[0], m[4], m[8], 0, m[1], m[5], m[9], 0, m[2], m[6], m[10], 0, m[3], m[7], m[11], 1], b * 16);
  }
}
