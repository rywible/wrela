// Rigid posing of the grazer's parts, for a field that is traced rather than skinned.
//
// Spike 01 skins an extracted mesh. A ray-marched field can't be skinned, so each part is rigid in
// one bone's frame: round cones get world-space endpoints, ellipsoids (torso, head) get the inverse
// bone transform so they and their noise are evaluated in rest space. The neck and tail, which
// spike 01 bends by skinning one cone across several bones, become chains of one cone per bone.
//
// The grazer's numbers and walk cycle come from spike 01's grazer.js, unchanged.

import {
  makeGrazer, pose, BONES, HEADER_FLOATS, PART_FLOATS,
  TORSO, NECK, HEAD, TAIL, LEG0, HOOF0, PELVIS, NECK0, HEAD_BONE, TAIL0, LEG_BONE0,
} from '../01-grazer/grazer.js';

export { makeGrazer };

// Per-instance buffer layout, in vec4s. Must match common.wgsl.
export const I = {
  MISC: 0, SEED: 1, BOUND: 2, TINV: 3, HINV: 6, TORSO: 9, HEAD: 13,
  TRAD: 17, HRAD: 19, MUZ: 20, TSPH: 22, HSPH: 24, CONES: 25, PLANES: 77,
};
export const STRIDE = 81;          // vec4s per instance
export const NCONES = 26;          // neck 4, tail 6, legs 12, hooves 4
export const NPARTS = 28;          // + torso, head
export const FBM_SUM = 1.875;      // 1 + 1/2 + 1/4 + 1/8: |fbm| ≤ this for 4 octaves of |noise| ≤ 1

const NECK_SEGS = 4, TAIL_SEGS = 6;

const lerp4 = (a, b, s) => a.map((x, i) => x + (b[i] - x) * s);

/** Everything about one individual that doesn't change per frame. */
export function makeInstance(seed) {
  const g = makeGrazer(seed);
  const P = g.params;
  const pv = (i, k) => Array.from(P.subarray(HEADER_FLOATS + i * PART_FLOATS + k * 4, HEADER_FLOATS + i * PART_FLOATS + k * 4 + 4));

  // Cones in rest space, each rigid in one bone: [a(xyz,r), b(xyz,r), bone, sole (hooves only)].
  const cones = [];
  for (let i = 0; i < NECK_SEGS; i++)
    cones.push({ a: lerp4(pv(NECK, 0), pv(NECK, 1), i / NECK_SEGS), b: lerp4(pv(NECK, 0), pv(NECK, 1), (i + 1) / NECK_SEGS), bone: NECK0 + i });
  for (let i = 0; i < TAIL_SEGS; i++)
    cones.push({ a: lerp4(pv(TAIL, 0), pv(TAIL, 1), i / TAIL_SEGS), b: lerp4(pv(TAIL, 0), pv(TAIL, 1), (i + 1) / TAIL_SEGS), bone: TAIL0 + i });
  for (let l = 0; l < 4; l++)
    for (let j = 0; j < 3; j++)
      cones.push({ a: pv(LEG0 + l * 3 + j, 0), b: pv(LEG0 + l * 3 + j, 1), bone: LEG_BONE0 + l * 4 + j });
  for (let l = 0; l < 4; l++)
    cones.push({ a: pv(HOOF0 + l, 0), b: pv(HOOF0 + l, 1), bone: LEG_BONE0 + l * 4 + 3, sole: pv(HOOF0 + l, 2)[0] });

  // Bound margins (README, "Why leaving parts out is exact").
  const k = P[0];
  const A = P[1] * FBM_SUM;
  const mT = k + pv(TORSO, 0)[3] / 4 + A;   // global blend + torso's inner blend + displacement
  const mH = k + pv(HEAD, 0)[3] / 4;        // global blend + head's inner blend
  const inflate = (r, m) => { const s = 1 + m / Math.min(r[0], r[1], r[2]); return [r[0] * s, r[1] * s, r[2] * s]; };
  const tr1 = inflate(pv(TORSO, 1), mT), tr2 = inflate(pv(TORSO, 3), mT), hr = inflate(pv(HEAD, 1), mH);

  return {
    seed, g, pv, cones, k, A, mH, tr1, tr2, hr,
    torso: [pv(TORSO, 0), pv(TORSO, 1), pv(TORSO, 2), pv(TORSO, 3)],
    head: [pv(HEAD, 0), pv(HEAD, 1), pv(HEAD, 2), pv(HEAD, 3)],
  };
}

// Column-major mat4 helpers on a palette Float32Array.
const xf = (m, o, p) => [
  m[o] * p[0] + m[o + 4] * p[1] + m[o + 8] * p[2] + m[o + 12],
  m[o + 1] * p[0] + m[o + 5] * p[1] + m[o + 9] * p[2] + m[o + 13],
  m[o + 2] * p[0] + m[o + 6] * p[1] + m[o + 10] * p[2] + m[o + 14],
];
/** Rows of the rigid inverse [Rᵀ | −Rᵀt]: world → rest. */
const invRows = (m, o) => {
  const t = [m[o + 12], m[o + 13], m[o + 14]];
  const rows = [];
  for (let i = 0; i < 3; i++) {
    const c = [m[o + i * 4], m[o + i * 4 + 1], m[o + i * 4 + 2]];
    rows.push([c[0], c[1], c[2], -(c[0] * t[0] + c[1] * t[1] + c[2] * t[2])]);
  }
  return rows;
};
const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

const PAL = new Float32Array(BONES * 16);

/** Poses one instance at time t and writes its STRIDE vec4s into `out` at float offset `off`.
 *  `place` = { x, y, z, yaw, phase, headDown }, as in spike 01. */
export function writeInstance(ins, place, t, out, off) {
  pose(ins.g, place, t, PAL, 0);
  const put = (vec4i, v) => out.set(v, off + vec4i * 4);
  const spheres = [];

  put(I.MISC, ins.g.params.subarray(0, 4));
  put(I.SEED, ins.g.params.subarray(4, 8));

  const tb = PELVIS * 16, hb = HEAD_BONE * 16;
  invRows(PAL, tb).forEach((r, i) => put(I.TINV + i, r));
  invRows(PAL, hb).forEach((r, i) => put(I.HINV + i, r));
  ins.torso.forEach((v, i) => put(I.TORSO + i, v));
  ins.head.forEach((v, i) => put(I.HEAD + i, v));
  put(I.TRAD, [...ins.tr1, 0]);
  put(I.TRAD + 1, [...ins.tr2, 0]);
  put(I.HRAD, [...ins.hr, 0]);

  // Muzzle capsule (world), inflated by the head's margin.
  const [, , ma, mb] = ins.head;
  const muzR = Math.max(ma[3], mb[3]) + ins.mH;
  const mA = xf(PAL, hb, ma), mB = xf(PAL, hb, mb);
  put(I.MUZ, [...mA, muzR]);
  put(I.MUZ + 1, [...mB, 0]);
  spheres.push([[(mA[0] + mB[0]) / 2, (mA[1] + mB[1]) / 2, (mA[2] + mB[2]) / 2], dist(mA, mB) / 2 + muzR]);

  // Ellipsoid bound spheres (world), for the binning passes.
  const sph = (bone, c, r) => { const s = [...xf(PAL, bone, c), Math.max(...r)]; spheres.push([s.slice(0, 3), s[3]]); return s; };
  put(I.TSPH, sph(tb, ins.torso[0], ins.tr1));
  put(I.TSPH + 1, sph(tb, ins.torso[2], ins.tr2));
  put(I.HSPH, sph(hb, ins.head[0], ins.hr));

  // Cones (world endpoints) and hoof sole planes.
  ins.cones.forEach((c, i) => {
    const o = c.bone * 16;
    const a = xf(PAL, o, c.a), b = xf(PAL, o, c.b);
    put(I.CONES + 2 * i, [...a, c.a[3]]);
    put(I.CONES + 2 * i + 1, [...b, c.b[3]]);
    const r = Math.max(c.a[3], c.b[3]) + ins.k;
    spheres.push([[(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2], dist(a, b) / 2 + r]);
    if (c.sole !== undefined) {
      const row = invRows(PAL, o)[1];      // rest-space y of a world point = dot(row.xyz, p) + row.w
      put(I.PLANES + (i - 22), [row[0], row[1], row[2], row[3] - c.sole]);
    }
  });

  // Instance bound: a sphere around every part's bound sphere.
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (const [c, r] of spheres) for (let a = 0; a < 3; a++) { lo[a] = Math.min(lo[a], c[a] - r); hi[a] = Math.max(hi[a], c[a] + r); }
  const C = [0, 1, 2].map(a => (lo[a] + hi[a]) / 2);
  let R = 0;
  for (const [c, r] of spheres) R = Math.max(R, dist(c, C) + r);
  put(I.BOUND, [...C, R]);
}
