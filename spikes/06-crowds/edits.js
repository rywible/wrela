// The edit schedule: a deterministic stream of edit events (craters, wall blasts, trench digs), the
// log they append to, and the CPU's share of re-cooking: the box of cells an event can change and
// the log entries that overlap it. Must agree with cook.wgsl's `reach` and `affected`.

import { rng, terrainHeight } from './crowd.js';

export const RMIN = [-48, -8, -48];
export const RDIM = [96, 40, 96];
export const MAXSKIP = 4.0;
export const MARGIN = 2.75;       // ≥ BAND + two half-diagonals: cells whose samples an edit can touch
export const EDIT_FLOATS = 8;     // two vec4s
export const EG = 2, EGD = [48, 20, 48];   // the analytic world's edit lists: 2 m cells (trace.wgsl)
export const AN_CLAMP = 2.0;               // the analytic world is clamped to this (trace.wgsl)

/** How far an edit reaches for the analytic world's lists: every point that world is evaluated at
 *  (outside, or at most 0.5 m inside, a surface) with the field clamped to AN_CLAMP. */
function listReach(e) {
  const core = e.op === 1 ? e.r + e.k + AN_CLAMP : Math.max(e.r + e.k, e.mat > 0 ? 1.9 * e.r : 0);
  return core + 0.6;
}

const WALL_A = [24, 27], WALL_B = [40, 11];   // the segment being blasted (world.wgsl's wall_pt 1 → 2)

/** How far an edit can change the cache from its centre (cook.wgsl's `reach`). */
export function reach(e) {
  let r = e.r + e.k;
  if (e.mat > 0 && e.op === 0) r = Math.max(r, 1.9 * e.r);
  if (e.op === 1) r += MAXSKIP;
  return r;
}

export class EditLog {
  constructor(seed = 6006, capacity = 4096) {
    this.r = rng(seed);
    this.prims = [];
    this.capacity = capacity;
    this.data = new Float32Array(capacity * EDIT_FLOATS);
    this.dig = 0;
    this.events = 0;
  }

  push(e) {
    if (this.prims.length >= this.capacity) throw new Error('edit log full');
    const i = this.prims.length;
    this.prims.push(e);
    this.data.set([e.x, e.y, e.z, e.r, e.op, e.k, e.mat, 0], i * EDIT_FLOATS);
  }

  /** The next event: appends its primitives and returns { kind, first, end }. */
  next() {
    const r = this.r;
    const first = this.prims.length;
    const u = r();
    let kind;
    if (u < 0.4) {
      kind = 'crater';
      const x = 20 + 18 * r(), z = -16 + 22 * r(), rad = 1.0 + 1.6 * r();
      this.push({ x, y: terrainHeight(x, z) + 0.3 * rad, z, r: rad, op: 0, k: 0.5, mat: 1 });
    } else if (u < 0.7) {
      kind = 'blast';
      const len = Math.hypot(WALL_B[0] - WALL_A[0], WALL_B[1] - WALL_A[1]);
      const dx = (WALL_B[0] - WALL_A[0]) / len, dz = (WALL_B[1] - WALL_A[1]) / len;
      const s = 2 + (len - 4) * r(), y = 1.0 + 5.0 * r(), rad = 0.8 + 0.7 * r();
      const x = WALL_A[0] + dx * s, z = WALL_A[1] + dz * s;
      this.push({ x, y, z, r: rad, op: 0, k: 0.15, mat: -1 });
      for (let j = 0; j < 3; j++) {
        const side = r() < 0.5 ? -1 : 1, off = 1.1 + rad * r() + 0.8 * r(), along = (r() - 0.5) * 2.0;
        const rx = x + dx * along - dz * side * off, rz = z + dz * along + dx * side * off;
        const rr = 0.25 + 0.25 * r();
        this.push({ x: rx, y: terrainHeight(rx, rz) + 0.1, z: rz, r: rr, op: 1, k: 0.1, mat: -1 });
      }
    } else {
      kind = 'dig';
      const lane = Math.floor(this.dig / 44), s = (this.dig % 44) * 0.45;
      const x0 = 24, z0 = -27 - 2.2 * lane, x1 = 44, z1 = -31 - 2.2 * lane;
      const len = Math.hypot(x1 - x0, z1 - z0);
      const x = x0 + (x1 - x0) * s / len, z = z0 + (z1 - z0) * s / len;
      this.push({ x, y: terrainHeight(x, z) + 0.15, z, r: 0.75, op: 0, k: 0.2, mat: 1 });
      this.dig++;
    }
    this.events++;
    return { kind, first, end: this.prims.length };
  }

  /** The cell box that edits [first, end) can change, and the log entries overlapping it. */
  batch(first, end) {
    const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    for (let i = first; i < end; i++) {
      const e = this.prims[i], R = reach(e) + MARGIN, c = [e.x, e.y, e.z];
      for (let a = 0; a < 3; a++) {
        lo[a] = Math.min(lo[a], Math.floor(c[a] - R - RMIN[a]));
        hi[a] = Math.max(hi[a], Math.ceil(c[a] + R - RMIN[a]));
      }
    }
    for (let a = 0; a < 3; a++) { lo[a] = Math.max(lo[a], 0); hi[a] = Math.min(hi[a], RDIM[a]); }
    const size = [0, 1, 2].map(a => Math.max(hi[a] - lo[a], 0));
    const relevant = this.relevantFor(lo, size, end);
    return { lo, size, cells: size[0] * size[1] * size[2], relevant };
  }

  /** For the analytic world: per 4 m cell, the log entries whose reach touches it, in log order.
   *  Returns (first, count) per cell and the flat index list. */
  grid() {
    const n = EGD[0] * EGD[1] * EGD[2];
    const lists = Array.from({ length: n }, () => []);
    for (let i = 0; i < this.prims.length; i++) {
      const e = this.prims[i], R = listReach(e), c = [e.x, e.y, e.z];
      const lo = [0, 1, 2].map(a => Math.max(0, Math.floor((c[a] - R - RMIN[a]) / EG)));
      const hi = [0, 1, 2].map(a => Math.min(EGD[a] - 1, Math.floor((c[a] + R - RMIN[a]) / EG)));
      for (let y = lo[1]; y <= hi[1]; y++) for (let z = lo[2]; z <= hi[2]; z++) for (let x = lo[0]; x <= hi[0]; x++) {
        const g = [x, y, z];
        let d2 = 0;
        for (let a = 0; a < 3; a++) {
          const b0 = g[a] * EG + RMIN[a], b1 = b0 + EG;
          const v = Math.max(b0 - c[a], 0, c[a] - b1);
          d2 += v * v;
        }
        if (d2 < R * R) lists[(y * EGD[2] + z) * EGD[0] + x].push(i);
      }
    }
    const info = new Uint32Array(2 * n);
    let total = 0;
    lists.forEach((l, k) => { info[2 * k] = total; info[2 * k + 1] = l.length; total += l.length; });
    const idx = new Uint32Array(Math.max(total, 1));
    lists.forEach((l, k) => idx.set(l, info[2 * k]));
    return { info, idx, total, max: Math.max(...lists.map(l => l.length)) };
  }

  /** Log entries [0, end) that can change anything in the cell box [lo, lo + size). */
  relevantFor(lo, size, end = this.prims.length) {
    const bmin = lo.map((v, a) => v + RMIN[a]), bmax = lo.map((v, a) => v + size[a] + RMIN[a]);
    const relevant = [];
    for (let i = 0; i < end; i++) {
      const e = this.prims[i], R = reach(e) + MARGIN;
      let d2 = 0;
      const c = [e.x, e.y, e.z];
      for (let a = 0; a < 3; a++) { const v = Math.max(bmin[a] - c[a], 0, c[a] - bmax[a]); d2 += v * v; }
      if (d2 < R * R) relevant.push(i);
    }
    return relevant;
  }
}
