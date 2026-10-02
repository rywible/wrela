// The world, mirrored in JS: hash, terrain, masks, trees and crown envelopes (the same functions as
// common.wgsl), plus the periodic leaf tiles, the 3D-texture bake and the density calibration.
// The GPU reads the leaf tiles from a table this file builds, so explicit leaves and the baked
// volumetric texture come from one definition.

export const TREE_C = 6.0;         // tree grid cell (m)
export const TREE_J = 1.5;         // trunk position jitter, ± (m)
export const CLEAR_R = 22.0;       // the clearing at the origin
export const UNDER_OCC = 0.35;     // understory occupancy (of the forest mask)
export const LEAF_P = 16;          // leaf tile period, in cells per axis
export const LEAF_K = 6;           // leaves per cell, per grid (two grids, offset by half a cell)
export const TEX_R = 8;            // texels per leaf cell
export const TEX_N = LEAF_P * TEX_R;   // 128
export const MIPS = 8;             // 128 → 1
export const CELL_VEC4 = 1 + 3 * LEAF_K;   // table vec4s per leaf cell (per grid)

/** Leaf species. Sizes are in leaf-cell units; `cell` is the cell size in metres. */
export const SPECIES = [
  { name: 'broadleaf', cell: 0.22, aMin: 0.14, aMax: 0.19, bRatio: 0.62, off: 0.15, ccMin: 0.355, ccMax: 0.645, egg: true },
  { name: 'conifer', cell: 0.10, aMin: 0.19, aMax: 0.26, bRatio: 0.30, off: 0.15, ccMin: 0.425, ccMax: 0.575, egg: false },
];

// ---- hash (pcg3d, Jarzynski & Olano 2020), bit-identical to common.wgsl -------------------------

export function pcg3(x, y, z) {
  x = (Math.imul(x >>> 0, 1664525) + 1013904223) >>> 0;
  y = (Math.imul(y >>> 0, 1664525) + 1013904223) >>> 0;
  z = (Math.imul(z >>> 0, 1664525) + 1013904223) >>> 0;
  x = (x + Math.imul(y, z)) >>> 0;
  y = (y + Math.imul(z, x)) >>> 0;
  z = (z + Math.imul(x, y)) >>> 0;
  x = (x ^ (x >>> 16)) >>> 0;
  y = (y ^ (y >>> 16)) >>> 0;
  z = (z ^ (z >>> 16)) >>> 0;
  x = (x + Math.imul(y, z)) >>> 0;
  y = (y + Math.imul(z, x)) >>> 0;
  z = (z + Math.imul(x, y)) >>> 0;
  return [x, y, z];
}
export const u01 = x => (x >>> 8) / 16777216;

// ---- terrain and masks ------------------------------------------------------------------------------

export function terrainH(x, z) {
  return 7.0 * Math.sin(0.0071 * x + 0.4) * Math.cos(0.0059 * z - 0.3)
       + 3.2 * Math.sin(0.017 * x - 0.013 * z + 1.7) * Math.cos(0.021 * z + 0.6)
       + 1.1 * Math.sin(0.043 * x + 2.1) * Math.sin(0.051 * z + 0.9)
       + 0.25 * Math.sin(0.13 * x + 0.17 * z);
}

const smoothstep = (a, b, x) => { const t = Math.min(Math.max((x - a) / (b - a), 0), 1); return t * t * (3 - 2 * t); };
const fract = x => x - Math.floor(x);

export function meadowField(x, z) {
  return Math.sin(0.0123 * x + 1.3) * Math.sin(0.0151 * z + 0.2) + 0.5 * Math.sin(0.031 * x - 0.027 * z + 0.4);
}
export function forestMask(x, z) {
  const r = Math.hypot(x, z);
  return smoothstep(CLEAR_R, CLEAR_R + 9.0, r) * (1.0 - smoothstep(0.95, 1.25, meadowField(x, z)));
}
export function coniferShare(x, z) {
  const s = 0.4 + 0.45 * Math.sin(0.0093 * x + 0.3) * Math.cos(0.0117 * z - 1.2) + 0.15 * Math.sin(0.041 * x + 0.033 * z);
  return Math.min(Math.max(s, 0.05), 0.92);
}

// ---- trees ------------------------------------------------------------------------------------------------

/** Tree slot 0 (canopy) or 1 (understory) of grid cell (ci, cj): the same function as tree_lite +
 *  tree_full in common.wgsl. */
export function treeAt(ci, cj, occupancy, slot = 0) {
  const h = pcg3(ci >>> 0, cj >>> 0, (0x2545F491 + Math.imul(slot, 0x9E3779B9)) >>> 0);
  const h2 = pcg3(h[1], h[2], h[0]);
  const h3 = pcg3(h2[2], h2[0], (h2[1] ^ 0x68E31DA4) >>> 0);
  const h4 = pcg3(h3[1], h3[2], (h3[0] + 77) >>> 0);
  const px = (ci + 0.5) * TREE_C + (u01(h[1]) - 0.5) * 2 * TREE_J;
  const pz = (cj + 0.5) * TREE_C + (u01(h[2]) - 0.5) * 2 * TREE_J;
  const occ = slot === 0 ? occupancy : UNDER_OCC;
  const exists = u01(h[0]) < occ * forestMask(px, pz);
  const kind = u01(h2[0]) < coniferShare(px, pz) ? 1 : 0;
  const size = u01(h2[1]), u = u01(h2[2]), v = u01(h3[0]);
  const t = { ci, cj, slot, exists, kind, size, px, pz, tint: u01(h3[1]), yaw: u01(h3[2]) * 6.2831853,
    off: [(h3[0] >>> 3) & 15, (h3[1] >>> 11) & 15, (h3[2] >>> 19) & 15],
    ph: [u01(h4[0]) * 6.2831853, u01(h4[1]) * 6.2831853, u01(h4[2]) * 6.2831853] };
  treeShape(t, u, v);
  t.base = terrainH(px, pz) - 0.2;
  return t;
}

export function treeShape(t, u, v) {
  const s = t.size;
  if (t.slot === 0) {
    if (t.kind === 0) { t.rh = 2.8 + 1.8 * u; t.rv = t.rh * (0.9 + 0.45 * v); t.height = 14 + 10 * s; t.cy = t.height - t.rv; t.r0 = 0.18 + 0.16 * s; }
    else { t.rh = 2.5 + 1.0 * u; t.height = 15 + 11 * s; t.cy = 1.0 + 2.0 * v; t.rv = t.height - t.cy; t.r0 = 0.16 + 0.14 * s; }
  } else {
    if (t.kind === 0) { t.rh = 1.2 + 1.3 * u; t.rv = t.rh * (0.85 + 0.4 * v); t.height = 3.5 + 5 * s; t.cy = t.height - t.rv; t.r0 = 0.05 + 0.06 * s; }
    else { t.rh = 0.9 + 1.0 * u; t.height = 3.0 + 5 * s; t.cy = 0.3 + 0.5 * v; t.rv = t.height - t.cy; t.r0 = 0.05 + 0.05 * s; }
  }
}

/** Crown envelope in the crown frame (metres, rotated by yaw). Returns [ef, profile]. Broadleaf crowns
 *  are a union of seven foliage clumps inside the (rh, rv, rh) ellipsoid; conifers are a tiered cone. */
export function crownEnvelope(t, lx, ly, lz) {
  const ph = t.ph;
  if (t.kind === 0) {
    const qx = lx / t.rh, qy = ly / t.rv, qz = lz / t.rh;
    const clump = (dx, dy, dz, sk) => sk - Math.hypot(qx - dx * (1 - sk), qy - dy * (1 - sk), qz - dz * (1 - sk));
    let ef = clump(0, 1, 0, 0.5 + 0.1 * fract(ph[1] * 1.618));
    let c = Math.cos(ph[0]), sn = Math.sin(ph[0]);
    const cr = Math.cos(1.2566371), sr = Math.sin(1.2566371);
    for (let k = 0; k < 5; k++) {
      const sk = 0.42 + 0.16 * fract(ph[1] * (k + 2) * 1.618);
      ef = Math.max(ef, clump(c * 0.92, 0.39, sn * 0.92, sk));
      const c2 = c * cr - sn * sr; sn = c * sr + sn * cr; c = c2;
    }
    ef = Math.max(ef, clump(0, -0.8, 0, 0.45 + 0.1 * fract(ph[2] * 1.618)));
    return [ef, smoothstep(0.0, 0.1, ef) * (1.0 - 0.3 * smoothstep(0.22, 0.45, ef))];
  }
  const hc = t.rv, f = ly / hc;
  const tier = Math.floor((hc - ly) / 1.3);
  const ft = fract((hc - ly) / 1.3);
  const saw = ft > 0.85 ? (1.0 - ft) / 0.15 * 0.85 : ft;
  const tv = 0.8 + 0.35 * fract(tier * 0.618 + ph[0]);
  const r = Math.hypot(lx, lz), rx = lx / Math.max(r, 1e-4), rz = lz / Math.max(r, 1e-4);
  const ta = tier * 2.4 + ph[1];
  const cth = rx * Math.cos(ta) + rz * Math.sin(ta);
  const lobe = 1.0 + 0.12 * (4 * cth * cth * cth - 3 * cth);
  const ra = (1.0 - f) * t.rh * (0.78 + 0.22 * saw) * tv * lobe;
  let ef = (ra - r) / t.rh;
  ef = Math.min(ef, Math.min(ly, hc - ly) / t.rh);
  return [ef, smoothstep(0.0, 0.08, ef) * (1.0 - 0.35 * smoothstep(0.2, 0.45, ef)) * (0.7 + 0.3 * saw)];
}

// ---- leaf tiles ----------------------------------------------------------------------------------------------

function norm3(v) { const l = Math.hypot(v[0], v[1], v[2]); return [v[0] / l, v[1] / l, v[2] / l]; }
function cross3(a, b) { return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]; }
const dot3 = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

/** One species' periodic leaf tile: per cell a presence draw and LEAF_K leaves, all in cell units. */
const GAP_WAVES = [
  [[1, 2, 0, 0.3], [2, 0, 1, 1.9], [0, 1, 3, 4.1], [3, 1, 2, 2.6]],
  [[2, 1, 0, 1.1], [0, 2, 1, 3.3], [1, 0, 3, 0.7], [2, 3, 1, 5.2]],
];

export function buildLeafTile(sp, kind) {
  const cells = [];
  const lerp = (a, b, t) => a + (b - a) * t;
  for (let grid = 0; grid < 2; grid++) {
    const go = 0.5 * grid;   // grid B's cells are centred on grid A's corners
    for (let z = 0; z < LEAF_P; z++) for (let y = 0; y < LEAF_P; y++) for (let x = 0; x < LEAF_P; x++) {
      const h = pcg3(x + 16 * kind + 64 * grid, y + 977, z + 3511);
      const hc = pcg3(h[2], h[0], h[1]);
      const leaves = [];
      for (let k = 0; k < LEAF_K; k++) {
        const a1 = pcg3((h[0] + Math.imul(k + 1, 0x9E3779B9)) >>> 0, (h[1] ^ (k * 7919)) >>> 0, (h[2] + k) >>> 0);
        const a2 = pcg3(a1[2], a1[0], (a1[1] ^ 0x85EBCA6B) >>> 0);
        const a = lerp(sp.aMin, sp.aMax, u01(a2[0] ^ a1[2]));
        const lo = a + 0.016;   // the leaf (and its flutter) stays inside its cell
        const c = [0, 1, 2].map(i => lo + (1 - 2 * lo) * u01(a1[i]));
        let n;
        if (kind === 0) n = norm3([(u01(a2[0]) - 0.5) * 2.4, 0.55 + 0.9 * u01(a2[1]), (u01(a2[2]) - 0.5) * 2.4]);
        else n = norm3([(u01(a2[0]) - 0.5) * 1.4, 1.0, (u01(a2[2]) - 0.5) * 1.4]);
        const th = u01(a2[1] ^ a1[0]) * 6.2831853;
        const r = [Math.cos(th), 0.25 * (u01(a1[1] ^ a2[2]) - 0.5), Math.sin(th)];
        const tv = norm3([r[0] - n[0] * dot3(n, r), r[1] - n[1] * dot3(n, r), r[2] - n[2] * dot3(n, r)]);
        leaves.push({ c, n, tv, a, b: a * sp.bRatio, tint: u01((a1[0] ^ a2[1]) >>> 0) });
      }
      // Gap patches: a smooth periodic field over the tile (whole waves per period) thins whole cells,
      // so crowns get holes and ragged edges at the 0.5–2 m scale instead of uniform speckle.
      let g = 0;
      for (const [kx, ky, kz, ph] of GAP_WAVES[kind]) g += Math.cos(2 * Math.PI * (kx * (x + 0.5 + go) + ky * (y + 0.5 + go) + kz * (z + 0.5 + go)) / LEAF_P + ph) / 4;
      cells.push({ grid, go, x, y, z, presence: u01(hc[1]), lf: smoothstep(-0.6, -0.2, g), leaves });
    }
  }
  return cells;
}

/** Octahedral encoding of a unit vector to [-1, 1]². */
function octEnc(n) {
  const l1 = Math.abs(n[0]) + Math.abs(n[1]) + Math.abs(n[2]);
  let x = n[0] / l1, y = n[1] / l1;
  if (n[2] < 0) { const ox = x; x = (1 - Math.abs(y)) * (ox >= 0 ? 1 : -1); y = (1 - Math.abs(ox)) * (y >= 0 ? 1 : -1); }
  return [x, y];
}
const u8 = v => Math.max(0, Math.min(255, Math.round(v * 255)));
const s16 = v => (Math.max(-32767, Math.min(32767, Math.round(v * 32767))) & 0xFFFF);

/** The GPU table, [species][grid][cell], LEAF_U32 words per cell, packed (12 bytes per leaf, so explicit
 *  leaves read a quarter of the memory a float table did):
 *  [0] presence (unorm16) | gap factor (unorm16) << 16
 *  per leaf: [centre xyz (unorm8, cell units) | a/0.3 (unorm8)], [normal, octahedral snorm16 × 2],
 *            [tangent, octahedral unorm8 × 2 | b/0.3 (unorm8) | tint (unorm8)]. */
export const LEAF_U32 = 1 + 3 * LEAF_K + 1;
export function leafTable(tiles) {
  const out = new Uint32Array(tiles.reduce((n, t) => n + t.length, 0) * LEAF_U32);
  let o = 0;
  for (const tile of tiles) for (const cell of tile) {
    out[o] = (Math.round(cell.presence * 65535) | (Math.round(cell.lf * 65535) << 16)) >>> 0;
    let w = o + 1;
    for (const L of cell.leaves) {
      out[w++] = (u8(L.c[0]) | (u8(L.c[1]) << 8) | (u8(L.c[2]) << 16) | (u8(L.a / 0.3) << 24)) >>> 0;
      const n = octEnc(L.n);
      out[w++] = (s16(n[0]) | (s16(n[1]) << 16)) >>> 0;
      const t = octEnc(L.tv);
      out[w++] = (u8(t[0] * 0.5 + 0.5) | (u8(t[1] * 0.5 + 0.5) << 8) | (u8(L.b / 0.3) << 16) | (u8(L.tint) << 24)) >>> 0;
    }
    o += LEAF_U32;
  }
  return out;
}

/** The leaves as the GPU sees them after packing (so the bake and the calibration use exactly the
 *  leaves the shader intersects). */
export function quantizeTile(tile) {
  const q8 = v => u8(v) / 255;
  const octDec = (x, y) => {
    let n = [x, y, 1 - Math.abs(x) - Math.abs(y)];
    if (n[2] < 0) n = [(1 - Math.abs(n[1])) * (n[0] >= 0 ? 1 : -1), (1 - Math.abs(n[0])) * (n[1] >= 0 ? 1 : -1), n[2]];
    return norm3(n);
  };
  for (const cell of tile) {
    cell.presence = Math.round(cell.presence * 65535) / 65535;
    cell.lf = Math.round(cell.lf * 65535) / 65535;
    for (const L of cell.leaves) {
      L.c = L.c.map(q8);
      L.a = q8(L.a / 0.3) * 0.3;
      L.b = q8(L.b / 0.3) * 0.3;
      L.tint = q8(L.tint);
      const n = octEnc(L.n);
      L.n = octDec(Math.max(-1, Math.round(n[0] * 32767) / 32767), Math.max(-1, Math.round(n[1] * 32767) / 32767));
      const t = octEnc(L.tv);
      L.tv = octDec(q8(t[0] * 0.5 + 0.5) * 2 - 1, q8(t[1] * 0.5 + 0.5) * 2 - 1);
    }
  }
  return tile;
}

function inLeaf(sp, x, y) {
  if (sp.egg) { const w = 1.0 - 0.3 * x; return x * x + (y / w) * (y / w) < 1.0; }
  return x * x + y * y < 1.0;
}

/** Ray against one cell's leaves, in cell-local coordinates (o relative to the cell's corner). */
function cellHit(sp, cell, o, d, tmin, tmax) {
  let best = Infinity;
  for (const L of cell.leaves) {
    const den = dot3(d, L.n);
    if (Math.abs(den) < 1e-9) continue;
    const t = ((L.c[0] - o[0]) * L.n[0] + (L.c[1] - o[1]) * L.n[1] + (L.c[2] - o[2]) * L.n[2]) / den;
    if (t <= tmin || t >= tmax || t >= best) continue;
    const q = [o[0] + d[0] * t - L.c[0], o[1] + d[1] * t - L.c[1], o[2] + d[2] * t - L.c[2]];
    const bv = cross3(L.n, L.tv);
    if (inLeaf(sp, dot3(q, L.tv) / L.a, dot3(q, bv) / L.b)) best = t;
  }
  return best;
}

/** Explicit transmittance (0 or 1) of a segment through the periodic tile (both grids), with thinning. */
function explicitClear(sp, tile, o, d, len, keep) {
  const N3 = LEAF_P ** 3;
  for (let grid = 0; grid < 2; grid++) {
    const go = 0.5 * grid;
    const oo = [o[0] - go, o[1] - go, o[2] - go];
    let cx = Math.floor(oo[0]), cy = Math.floor(oo[1]), cz = Math.floor(oo[2]);
    const step = d.map(v => (v > 0 ? 1 : -1));
    const tDelta = d.map(v => (Math.abs(v) < 1e-12 ? Infinity : Math.abs(1 / v)));
    const tMax = [0, 1, 2].map(i => {
      if (Math.abs(d[i]) < 1e-12) return Infinity;
      const c = [cx, cy, cz][i];
      return ((d[i] > 0 ? c + 1 : c) - oo[i]) / d[i];
    });
    let t0 = 0;
    for (let guard = 0; guard < 4096; guard++) {
      const t1 = Math.min(tMax[0], tMax[1], tMax[2]);
      const m = v => ((v % LEAF_P) + LEAF_P) % LEAF_P;
      const cell = tile[grid * N3 + m(cx) + LEAF_P * m(cy) + LEAF_P * LEAF_P * m(cz)];
      if (cell.presence < keep * cell.lf) {
        const lo = [oo[0] - cx, oo[1] - cy, oo[2] - cz];
        if (cellHit(sp, cell, lo, d, Math.max(t0, 0) - 1e-6, Math.min(t1, len) + 1e-6) < Infinity) return 0;
      }
      if (t1 >= len) break;
      t0 = t1;
      if (tMax[0] <= tMax[1] && tMax[0] <= tMax[2]) { cx += step[0]; tMax[0] += tDelta[0]; }
      else if (tMax[1] <= tMax[2]) { cy += step[1]; tMax[1] += tDelta[1]; }
      else { cz += step[2]; tMax[2] += tDelta[2]; }
    }
  }
  return 1;
}

// ---- the volumetric texture: leaf-area density, density-weighted normal and tint ----------------------------------

/** Bakes one tile into TEX_N³ texels × 4 channels (R area density in cell⁻¹, G,B density × folded normal
 *  x,z, A density × tint), by depositing points sampled on every leaf with trilinear splats. Returns the
 *  full mip chain as Float32Arrays. */
export function bakeTile(sp, tile) {
  const N = TEX_N, data = new Float32Array(N * N * N * 4);
  const NU = 16, NV = 10;
  for (const cell of tile) {
    const x = cell.x + cell.go, y = cell.y + cell.go, z = cell.z + cell.go;
    for (const L of cell.leaves) {
      const bv = cross3(L.n, L.tv);
      const nf = L.n[1] < 0 ? L.n.map(v => -v) : L.n;
      const dA = (2 * L.a / NU) * (2 * L.b / NV) * cell.lf;
      for (let iu = 0; iu < NU; iu++) for (let iv = 0; iv < NV; iv++) {
        const lu = -1 + (iu + 0.5) * 2 / NU, lv = -1 + (iv + 0.5) * 2 / NV;
        if (!inLeaf(sp, lu, lv)) continue;
        const px = (x + L.c[0] + L.tv[0] * lu * L.a + bv[0] * lv * L.b) * TEX_R - 0.5;
        const py = (y + L.c[1] + L.tv[1] * lu * L.a + bv[1] * lv * L.b) * TEX_R - 0.5;
        const pz = (z + L.c[2] + L.tv[2] * lu * L.a + bv[2] * lv * L.b) * TEX_R - 0.5;
        const ix = Math.floor(px), iy = Math.floor(py), iz = Math.floor(pz);
        const fx = px - ix, fy = py - iy, fz = pz - iz;
        for (let c = 0; c < 8; c++) {
          const ox = c & 1, oy = (c >> 1) & 1, oz = c >> 2;
          const w = (ox ? fx : 1 - fx) * (oy ? fy : 1 - fy) * (oz ? fz : 1 - fz) * dA;
          const tx = (ix + ox + N) % N, ty = (iy + oy + N) % N, tz = (iz + oz + N) % N;
          const o = 4 * (tx + N * (ty + N * tz));
          data[o] += w; data[o + 1] += w * nf[0]; data[o + 2] += w * nf[2]; data[o + 3] += w * L.tint;
        }
      }
    }
  }
  const vol = 1 / (TEX_R * TEX_R * TEX_R);
  for (let i = 0; i < data.length; i++) data[i] /= vol;
  const mips = [data];
  for (let m = 1; m < MIPS; m++) {
    const n = N >> m, src = mips[m - 1], sn = n * 2, dst = new Float32Array(n * n * n * 4);
    for (let z = 0; z < n; z++) for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
      const o = 4 * (x + n * (y + n * z));
      for (let c = 0; c < 8; c++) {
        const so = 4 * ((2 * x + (c & 1)) + sn * ((2 * y + ((c >> 1) & 1)) + sn * (2 * z + (c >> 2))));
        for (let k = 0; k < 4; k++) dst[o + k] += src[so + k] / 8;
      }
    }
    mips.push(dst);
  }
  return mips;
}

function sampleMip(mip, n, x, y, z) {   // trilinear, periodic, coordinates in texels of this mip
  x -= 0.5; y -= 0.5; z -= 0.5;
  const ix = Math.floor(x), iy = Math.floor(y), iz = Math.floor(z);
  const fx = x - ix, fy = y - iy, fz = z - iz;
  let r = 0;
  for (let c = 0; c < 8; c++) {
    const ox = c & 1, oy = (c >> 1) & 1, oz = c >> 2;
    const w = (ox ? fx : 1 - fx) * (oy ? fy : 1 - fy) * (oz ? fz : 1 - fz);
    const tx = ((ix + ox) % n + n) % n, ty = ((iy + oy) % n + n) % n, tz = ((iz + oz) % n + n) % n;
    r += w * mip[4 * (tx + n * (ty + n * tz))];
  }
  return r;
}

// ---- calibration ----------------------------------------------------------------------------------------------------

function rng(seed) {
  let s = seed >>> 0;
  return () => { s = (Math.imul(s, 1664525) + 1013904223) >>> 0; return u01(pcg3(s, s ^ 0x9E3779B9, 77)[0]); };
}

/** Random direction with |d.y| in [lo, hi). */
function dirInBand(r, lo, hi) {
  const y = (lo + (hi - lo) * r()) * (r() < 0.5 ? -1 : 1);
  const s = Math.sqrt(1 - y * y), a = r() * 6.2831853;
  return [s * Math.cos(a), y, s * Math.sin(a)];
}

export const BANDS = [[0, 1 / 3], [1 / 3, 2 / 3], [2 / 3, 1]];

/** Factors c[mip][band] so that, for random segments through the tile with clusters kept at `keep`,
 *  the volume's mean transmittance exp(−c · keep · Σ R dt) (steps of two texels at that mip, random
 *  phase, as the shader marches) equals the explicit leaves' mean transmittance. */
export function calibrateTile(sp, tile, mips, { rays = 3000, len = 16, keep = 0.7, seed = 1 } = {}) {
  const out = [], target = [], segs = [];
  for (let b = 0; b < BANDS.length; b++) {
    const r = rng(seed * 97 + b * 13 + 5);
    const list = [];
    let clear = 0;
    for (let i = 0; i < rays; i++) {
      const o = [r() * LEAF_P, r() * LEAF_P, r() * LEAF_P];
      const d = dirInBand(r, BANDS[b][0], BANDS[b][1]);
      clear += explicitClear(sp, tile, o, d, len, keep);
      list.push({ o, d, ph: r() });
    }
    target.push(clear / rays);
    segs.push(list);
  }
  for (let m = 0; m < MIPS; m++) {
    const n = TEX_N >> m, dt = 2 * (1 << m) / TEX_R;   // two texels of this mip, in cells
    // A step longer than half the segment can't be calibrated this way; such mips reuse the last factor.
    if (dt > len / 2) { out.push(out[m - 1].slice()); continue; }
    const row = [];
    for (let b = 0; b < BANDS.length; b++) {
      // Samples at (i + phase)·dt with weight dt: unbiased for a uniform phase, as in the shader.
      const X = segs[b].map(({ o, d, ph }) => {
        let x = 0;
        for (let t = ph * dt; t < len; t += dt) {
          x += sampleMip(mips[m], n, (o[0] + d[0] * t) / LEAF_P * n, (o[1] + d[1] * t) / LEAF_P * n, (o[2] + d[2] * t) / LEAF_P * n);
        }
        return x * keep * dt;
      });
      const T = target[b];
      let lo = 0, hi = 4;
      for (let it = 0; it < 40; it++) {
        const c = (lo + hi) / 2;
        let s = 0;
        for (const x of X) s += Math.exp(-c * x);
        if (s / X.length > T) lo = c; else hi = c;
      }
      row.push((lo + hi) / 2);
    }
    out.push(row);
  }
  return { c: out, target };
}

/** The far level's extinction (m⁻¹) per species, so that crowns keep their mean opacity across the
 *  volume → far boundary. Sample crowns are drawn as the volumetric texture exactly as the shader
 *  marches it at that boundary (envelope entry, step max(two texels, chord / volSteps) at the mip the
 *  footprint and step select, random phase, empty-space skips, the calibrated factor), over bundles
 *  of parallel rays; the far level is a homogeneous ellipsoid (broadleaf) or cone (conifer). */
export function calibrateFar(kind, mips, cTable, shapeScale, { fpT = 0.5, volSteps = 16, trees = 12, rays = 800, seed = 3 } = {}) {
  const r = rng(seed + kind * 101);
  const sp = SPECIES[kind];
  const texel0 = sp.cell / TEX_R;
  const samples = [];
  let found = 0;
  const cAt = (m, band) => { const m0 = Math.min(Math.floor(m), MIPS - 1), m1 = Math.min(m0 + 1, MIPS - 1), f = m - m0; return cTable[m0][band] * (1 - f) + cTable[m1][band] * f; };
  const dens = (m, lx, ly, lz) => {
    const m0 = Math.min(Math.floor(m), MIPS - 1), m1 = Math.min(m0 + 1, MIPS - 1), f = m - m0;
    const a = sampleMip(mips[m0], TEX_N >> m0, lx / LEAF_P * (TEX_N >> m0), ly / LEAF_P * (TEX_N >> m0), lz / LEAF_P * (TEX_N >> m0));
    const b = sampleMip(mips[m1], TEX_N >> m1, lx / LEAF_P * (TEX_N >> m1), ly / LEAF_P * (TEX_N >> m1), lz / LEAF_P * (TEX_N >> m1));
    return a * (1 - f) + b * f;
  };
  for (let ci = -60; found < trees && ci < 60; ci++) {
    const t = treeAt(ci, 17 + ci % 5, 1.0);
    if (!t.exists || t.kind !== kind) continue;
    found++;
    const lip = kind === 0 ? Math.min(t.rh, t.rv) : 0.5 * t.rh;
    for (const elev of [0.0, 0.6]) {
      for (let i = 0; i < rays / 2; i++) {
        const az = r() * 6.2831853, ce = Math.cos(elev);
        const d = [ce * Math.cos(az), -Math.sin(elev), ce * Math.sin(az)];
        const R = kind === 0 ? Math.max(t.rh, t.rv) * 1.05 : Math.max(t.rh * 1.4, t.rv * 0.6);
        const up = Math.abs(d[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0];
        const e1 = norm3(cross3(d, up)), e2 = cross3(d, e1);
        const u = (r() * 2 - 1) * R, v = (r() * 2 - 1) * R;
        const cyw = kind === 0 ? 0 : t.rv * 0.4;
        const o = [e1[0] * u + e2[0] * v - d[0] * 3 * R, cyw + e1[1] * u + e2[1] * v - d[1] * 3 * R, e1[2] * u + e2[2] * v - d[2] * 3 * R];
        const band = Math.min(2, Math.floor(Math.abs(d[1]) * 3));
        const P = s => [o[0] + d[0] * s, o[1] + d[1] * s, o[2] + d[2] * s];
        // The crown's bound chord, as crown_iv gives it.
        const iv = kind === 0 ? ellipsoidIv(o, d, t.rh, t.rv) : coneIv(o, d, t.rh * 1.3, t.rv);
        let opacityVol = 0;
        if (iv) {
          const [ta, tb] = iv;
          const mip = Math.min(7, Math.max(Math.log2(Math.max(fpT / texel0, 1)), Math.log2(Math.max((tb - ta) / volSteps / (2 * texel0), 1))));
          const dt = 2 * texel0 * 2 ** mip;
          const k = cAt(mip, band) / sp.cell;
          // Envelope entry (env_entry, margin 2.2 cells), then the march with its empty-space skips.
          let va = ta;
          for (let it = 0; it < 10; it++) {
            const gap = -crownEnvelope(t, ...P(va))[0] * lip - 2.2 * sp.cell;
            if (gap <= 0) break;
            va += gap;
            if (va >= tb) break;
          }
          let tau = 0;
          for (let tt = va + r() * dt, n = 0; tt < tb && n < 48; tt += dt, n++) {
            const p = P(tt);
            const [ef, pr] = crownEnvelope(t, ...p);
            if (ef > 0) tau += k * pr * dt * dens(mip, p[0] / sp.cell + t.off[0], p[1] / sp.cell + t.off[1], p[2] / sp.cell + t.off[2]);
            else tt += Math.max(-ef * lip - dt, 0);
          }
          opacityVol = 1 - Math.exp(-tau);
        }
        const chord = kind === 0 ? ellipsoidChord(o, d, t.rh * shapeScale, t.rv * shapeScale) : coneChord(o, d, t.rh * shapeScale, t.rv);
        samples.push({ opacityVol, chord, elev });
      }
    }
  }
  // One extinction per view elevation (horizontal, 34° down): the volume's own extinction depends on
  // the ray's elevation through the calibration bands, so a single isotropic value can't match both.
  const fit = list => {
    const target = list.reduce((s, x) => s + x.opacityVol, 0) / list.length;
    let lo = 0, hi = 20;
    for (let it = 0; it < 50; it++) {
      const sg = (lo + hi) / 2;
      const m = list.reduce((s, x) => s + (1 - Math.exp(-sg * x.chord)), 0) / list.length;
      if (m < target) lo = sg; else hi = sg;
    }
    return { sigma: (lo + hi) / 2, target };
  };
  const h = fit(samples.filter(x => x.elev === 0)), v = fit(samples.filter(x => x.elev !== 0));
  return { sigma: h.sigma, sigmaDown: v.sigma, meanOpacity: (h.target + v.target) / 2, trees: found };
}

function ellipsoidIv(o, d, rh, rv) {
  const oo = [o[0] / rh, o[1] / rv, o[2] / rh], dd = [d[0] / rh, d[1] / rv, d[2] / rh];
  const a = dot3(dd, dd), b = dot3(oo, dd), c = dot3(oo, oo) - 1;
  const disc = b * b - a * c;
  if (disc <= 0) return null;
  const sq = Math.sqrt(disc);
  return [(-b - sq) / a, (-b + sq) / a];
}

/** First and last points inside a solid cone (base radius rb at y = 0, apex at y = hc), by sampling. */
function coneIv(o, d, rb, hc) {
  let a = null, b = null;
  const L = 6 * Math.max(rb, hc), dt = 0.02;
  for (let s = 0; s < L; s += dt) {
    const y = o[1] + d[1] * s;
    if (y < 0 || y > hc) continue;
    if (Math.hypot(o[0] + d[0] * s, o[2] + d[2] * s) < rb * (1 - y / hc)) { if (a === null) a = s; b = s; }
  }
  return a === null ? null : [a, b];
}

function ellipsoidChord(o, d, rh, rv) {
  const oo = [o[0] / rh, o[1] / rv, o[2] / rh], dd = [d[0] / rh, d[1] / rv, d[2] / rh];
  const a = dot3(dd, dd), b = dot3(oo, dd), c = dot3(oo, oo) - 1;
  const disc = b * b - a * c;
  if (disc <= 0) return 0;
  return 2 * Math.sqrt(disc) / a;
}

/** Chord through a solid cone: base radius rb at y = 0, apex at y = hc (crown frame). */
function coneChord(o, d, rb, hc) {
  // Sample finely; this runs once at start-up.
  let n = 0;
  const L = 6 * Math.max(rb, hc), dt = 0.02;
  for (let s = 0; s < L; s += dt) {
    const y = o[1] + d[1] * s;
    if (y < 0 || y > hc) continue;
    const rr = rb * (1 - y / hc);
    if (Math.hypot(o[0] + d[0] * s, o[2] + d[2] * s) < rr) n++;
  }
  return n * dt;
}

/** Float32 → IEEE half, for rgba16float uploads. */
export function toHalf(f32) {
  const out = new Uint16Array(f32.length);
  const fv = new Float32Array(1), iv = new Uint32Array(fv.buffer);
  for (let i = 0; i < f32.length; i++) {
    fv[0] = f32[i];
    const x = iv[0], sign = (x >>> 16) & 0x8000;
    let e = ((x >>> 23) & 0xff) - 127 + 15, m = x & 0x7fffff;
    if (e <= 0) {
      if (e < -10) { out[i] = sign; continue; }
      m = (m | 0x800000) >> (1 - e);
      out[i] = sign | ((m + 0x1000) >> 13);
    } else if (e >= 31) {
      out[i] = sign | 0x7c00;
    } else {
      const h = sign | (e << 10) | (m >> 13);
      out[i] = (m & 0x1000) ? h + 1 : h;
    }
  }
  return out;
}

/** Trees whose trunk lies within `radius` of (x, z). */
export function countTrees(x, z, radius, occupancy, slot = 0) {
  let n = 0;
  const r = Math.ceil(radius / TREE_C) + 1;
  const cx = Math.floor(x / TREE_C), cz = Math.floor(z / TREE_C);
  for (let i = cx - r; i <= cx + r; i++) for (let j = cz - r; j <= cz + r; j++) {
    const t = treeAt(i, j, occupancy, slot);
    if (t.exists && Math.hypot(t.px - x, t.pz - z) <= radius) n++;
  }
  return n;
}
