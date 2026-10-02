// Content for spike 11: the courtyard, the bodies and their walk cycle, the cloth sheets and their
// constraints, the hair guides, the pins that tie them to the bodies, and the cameras.
// This plays the game's role (content and animation). It isn't compiler output.
//
// Buffer layouts here must match common.wgsl.

export const NPARTS = 14;
export const CHAR_STRIDE = 36;      // vec4s per character (common.wgsl C_*)
export const K_BLEND = 0.035;       // smooth-union radius between a body's parts
export const MAX_CHARS = 64;
export const MAX_SHEETS = 64;

// Collision groups and kinds (common.wgsl G_*, KIND_*).
export const G_NONE = 0xffff, G_TOWER = 0xfffe;
export const KIND_CLOTHES = 0, KIND_CAPE = 1, KIND_BANNER = 2, KIND_HAIR = 3, KIND_FLAG = 4;
const NONE = 0xffffffff;

export const TOWER = { x: 3.0, z: 16.0, r: 3.4, top: 13.0 };

export const HAIR = {
  guides: 128, perGuide: 16, length: 0.62,     // simulated guides
  segsPerStrand: 30,                            // render segments per strand (Catmull-Rom ×2)
  radius2048: 0.0006,                           // wisp radius at 2,048 strands; scaled by 2048/N
  dims: [64, 96, 64],                           // density grid
};

export function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const mul = (a, s) => [a[0] * s, a[1] * s, a[2] * s];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const len = a => Math.hypot(a[0], a[1], a[2]);
const norm = a => mul(a, 1 / len(a));
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
export const v3 = { add, sub, mul, dot, len, norm, cross };

// ---- bodies ----------------------------------------------------------------------------------------

const PALETTE = {
  robes: [[0.42, 0.09, 0.06], [0.10, 0.17, 0.36], [0.45, 0.33, 0.12], [0.55, 0.50, 0.40], [0.17, 0.27, 0.12],
          [0.30, 0.12, 0.25], [0.50, 0.24, 0.08], [0.20, 0.30, 0.34], [0.62, 0.56, 0.44], [0.28, 0.20, 0.13]],
  skin: [[0.62, 0.42, 0.30], [0.45, 0.28, 0.18], [0.70, 0.50, 0.38], [0.30, 0.18, 0.11], [0.56, 0.36, 0.24]],
  hair: [[0.05, 0.035, 0.025], [0.20, 0.10, 0.04], [0.30, 0.22, 0.12], [0.02, 0.02, 0.02], [0.35, 0.33, 0.30]],
  trousers: [[0.10, 0.08, 0.06], [0.16, 0.13, 0.09], [0.08, 0.09, 0.10]],
};

/** One character's static description. `hero` stands still; NPCs walk circles round the square. */
function makeChar(i, hero, r, lane) {
  const pick = a => a[Math.floor(r() * a.length)];
  const c = {
    i, hero,
    scale: hero ? 1.0 : 0.94 + 0.12 * r(),
    garment: hero ? [0.09, 0.07, 0.055] : pick(PALETTE.robes).map(x => x * 0.55),
    trousers: hero ? [0.05, 0.045, 0.04] : pick(PALETTE.trousers),
    skin: hero ? [0.62, 0.43, 0.32] : pick(PALETTE.skin),
    hair: hero ? [0.30, 0.17, 0.07] : pick(PALETTE.hair),
    scalp: hero ? 1 : (r() < 0.75 ? 1 : 0),
  };
  if (hero) {
    Object.assign(c, { x: -0.35, z: -2.0, heading: 0.12, walk: 0 });
  } else {
    Object.assign(c, lane, { speed: (0.85 + 0.35 * r()) * lane.dir, phase0: r() });
  }
  return c;
}

/** The courtyard's cast: the hero first, then `npcs` walkers on concentric lanes. */
export function makeCast(npcs) {
  const r = rng(1234);
  const cast = [makeChar(0, true, r)];
  const lanes = npcs <= 20 ? 5 : 8;
  const per = Math.ceil(npcs / lanes);
  const centre = [1.6, 5.6];
  for (let k = 0; k < npcs; k++) {
    const l = k % lanes, j = Math.floor(k / lanes);
    const radius = 2.2 + l * (4.4 / (lanes - 1));
    const lane = { cx: centre[0], cz: centre[1], radius, a0: (j / per) * 2 * Math.PI + l * 0.9, dir: l % 2 ? 1 : -1 };
    cast.push(makeChar(k + 1, false, r, lane));
  }
  return cast;
}

/** Body frame at time t: root on the ground, heading, gait phase and how much it walks. */
export function bodyFrame(c, t) {
  if (c.hero) {
    const sway = 0.012 * Math.sin(t * 1.3);
    return { root: [c.x + sway, 0, c.z], heading: c.heading + 0.05 * Math.sin(t * 0.37), phase: 0, walk: 0, t };
  }
  const w = c.speed / c.radius;
  const a = c.a0 + w * t;
  const root = [c.cx + c.radius * Math.cos(a), 0, c.cz + c.radius * Math.sin(a)];
  const tangent = [-Math.sin(a) * Math.sign(w), 0, Math.cos(a) * Math.sign(w)];
  const heading = Math.atan2(tangent[0], tangent[2]);
  const phase = (c.phase0 + Math.abs(c.speed) * t / (1.35 * c.scale)) % 1;
  return { root, heading, phase, walk: 1, t };
}

function axes(heading) {
  const f = [Math.sin(heading), 0, Math.cos(heading)];
  const rt = [Math.cos(heading), 0, -Math.sin(heading)];
  return { f, r: rt, u: [0, 1, 0] };
}

/** Local (x right, y up, z forward) → world, for one body frame. */
export function toWorld(fr, s, p) {
  const ax = axes(fr.heading);
  return add(fr.root, add(add(mul(ax.r, p[0] * s), mul(ax.u, p[1] * s)), mul(ax.f, p[2] * s)));
}

const swing = (L, a) => [0, -L * Math.cos(a), L * Math.sin(a)];

/** The 14 rigid parts in body-local coordinates: [a, r1, b, r2]. */
export function localParts(fr) {
  const a = 2 * Math.PI * fr.phase, w = fr.walk;
  const bob = w ? 0.022 * Math.cos(2 * a) - 0.01 : 0.004 * Math.sin(fr.t * 1.9);
  const lean = w * 0.04;
  const thighL = w * 0.42 * Math.sin(a), thighR = -w * 0.42 * Math.sin(a);
  const kneeL = 0.06 + w * 0.8 * Math.max(0, Math.cos(a)) ** 2;
  const kneeR = 0.06 + w * 0.8 * Math.max(0, Math.cos(a + Math.PI)) ** 2;
  const armL = -w * 0.32 * Math.sin(a) + (w ? 0 : 0.08), armR = w * 0.32 * Math.sin(a) + (w ? 0 : 0.05);
  const elbowL = 0.25 + w * 0.12 * Math.max(0, -Math.sin(a)), elbowR = 0.25 + w * 0.12 * Math.max(0, Math.sin(a));
  const y = v => v + bob;
  const parts = [];
  const push = (pa, r1, pb, r2) => parts.push([pa, r1, pb, r2]);
  // Torso: two cones side by side, so it's wider than deep.
  push([-0.055, y(0.95), 0], 0.115, [-0.068, y(1.33), lean * 0.3], 0.125);
  push([0.055, y(0.95), 0], 0.115, [0.068, y(1.33), lean * 0.3], 0.125);
  push([0, y(1.39), lean * 0.4], 0.056, [0, y(1.52), lean * 0.5 + 0.01], 0.05);                 // neck
  push([0, y(1.585), lean * 0.6 + 0.018], 0.091, [0, y(1.645), lean * 0.6 + 0.004], 0.098);      // head
  for (const [side, arm, elbow] of [[-1, armL, elbowL], [1, armR, elbowR]]) {
    const sh = [side * 0.196, y(1.37), lean * 0.3];
    const el = add(add(sh, [side * 0.035, 0, 0]), swing(0.285, arm));
    const wr = add(add(el, [side * 0.01, 0, 0]), swing(0.255, arm + elbow));
    push(sh, 0.054, el, 0.043);
    push(el, 0.041, wr, 0.036);
  }
  for (const [side, th, kn] of [[-1, thighL, kneeL], [1, thighR, kneeR]]) {
    const hip = [side * 0.092, y(0.93), 0];
    const knee = add(hip, swing(0.44, th));
    const ankle = add(knee, swing(0.43, th - kn));
    push(hip, 0.078, knee, 0.053);
    push(knee, 0.051, ankle, 0.04);
  }
  // Parts 4–7 are arms (L upper, L fore, R upper, R fore), 8–11 legs (L thigh, L shin, R thigh, R shin).
  for (const k of [9, 11]) {
    const ankle = parts[k][2];
    push(ankle, 0.042, add(ankle, [0, -0.025, 0.15]), 0.036);                                   // feet
  }
  return parts;
}

/** Writes one character's record (CHAR_STRIDE vec4s) at `off`, posed at frame `fr`. */
export function writeChar(c, fr, out, off) {
  const s = c.scale;
  const lp = localParts(fr);
  const ax = axes(fr.heading);
  const lo = [1e9, 1e9, 1e9], hi = [-1e9, -1e9, -1e9];
  for (let k = 0; k < NPARTS; k++) {
    const [pa, r1, pb, r2] = lp[k];
    const A = toWorld(fr, s, pa), B = toWorld(fr, s, pb);
    out.set([...A, r1 * s, ...B, r2 * s], off + (8 + 2 * k) * 4);
    const m = Math.max(r1, r2) * s + K_BLEND;
    for (let i = 0; i < 3; i++) {
      lo[i] = Math.min(lo[i], A[i] - m, B[i] - m);
      hi[i] = Math.max(hi[i], A[i] + m, B[i] + m);
    }
  }
  out.set([...lo, K_BLEND, ...hi, c.hero ? 1 : 0], off);
  out.set([...c.garment, 0.75, ...c.trousers, 0.85, ...c.skin, 0.5, ...c.hair, c.scalp], off + 8);
  out.set([...ax.f, 0, ...ax.r, 0], off + 24);
  return { lo, hi };
}

// ---- cloth -------------------------------------------------------------------------------------------

/**
 * A sheet is a grid of nu × nv particles (u across, v down). Tubes wrap in u. `place(u, v)` gives the
 * rest position in world space at t = 0, `pin(u, v)` returns a pin descriptor or null, and `rest`
 * gives the rest spacing so the cloth can be cut wider at the hem than where it's pinned.
 */
function sheet(kind, nu, nv, wrap, opts) {
  return { kind, nu, nv, wrap, ...opts };
}

const scaled = (n, s) => Math.max(4, Math.round((n - 1) * s) + 1);

/** Every sheet in the scene, at resolution scale `s` (1 = default). */
export function makeSheets(cast, s) {
  const sheets = [];
  const hero = cast[0];
  // The hero's cape: pinned along an arc round the back of the neck, from shoulder to shoulder.
  {
    const nu = scaled(24, s), nv = scaled(26, s);
    const top = u => { const a = Math.PI * (0.04 + 0.92 * u); return [-0.205 * Math.cos(a), 1.445, -0.03 - 0.115 * Math.sin(a)]; };
    const topW = 0.57, botW = 1.12, L = 1.27;
    sheets.push(sheet(KIND_CAPE, nu, nv, false, {
      char: 0, group: 0, length: L,
      local: (u, v) => { const t = top(u); return [t[0] * (1 + 1.1 * v), t[1] - L * v, t[2] - 0.05 * v - 0.06 * Math.sin(Math.PI * v)]; },
      pinned: (u, v) => v === 0,
      restU: v => (topW + (botW - topW) * v) / (nu - 1),
      restV: L / (nv - 1),
      mat: { front: [0.30, 0.035, 0.03], back: [0.55, 0.42, 0.20], accent: [0.62, 0.48, 0.18], pattern: 1, rough: 0.8 },
    }));
  }
  // NPC robes (to the ankle) and tunics (to the knee): tubes pinned round the shoulders.
  for (let k = 1; k < cast.length; k++) {
    const c = cast[k];
    const robe = k % 3 !== 0;
    const nu = scaled(28, s), nv = scaled(robe ? 22 : 14, s);
    const L = robe ? 1.28 : 0.8;
    const top = [0.215, 0.13], hem = robe ? [0.33, 0.30] : [0.28, 0.245];
    const ring = (u, v) => {
      const a = 2 * Math.PI * u;
      const rx = top[0] + (hem[0] - top[0]) * v, rz = top[1] + (hem[1] - top[1]) * v;
      return [rx * Math.cos(a), 1.435 - L * v, -0.01 + rz * Math.sin(a)];
    };
    const perim = v => {
      const rx = top[0] + (hem[0] - top[0]) * v, rz = top[1] + (hem[1] - top[1]) * v;
      return Math.PI * (3 * (rx + rz) - Math.sqrt((3 * rx + rz) * (rx + 3 * rz)));
    };
    const r = rng(777 + k);
    const k0 = r() * 6.28;
    const base = PALETTE.robes[Math.floor(r() * PALETTE.robes.length)];
    sheets.push(sheet(KIND_CLOTHES, nu, nv, true, {
      char: k, group: k, length: L,
      local: (u, v) => {
        const p = ring(u, v);
        const a = 2 * Math.PI * u, k = 0.06 * v * (0.6 * Math.sin(a * 7 + 1.3 * k0) + 0.4 * Math.sin(a * 11 + 0.7));
        return [p[0] + k * Math.cos(a), p[1], p[2] + k * Math.sin(a)];
      },
      pinned: (u, v) => v === 0,
      restU: v => Math.max(perim(v), perim(1) * (0.72 + 0.28 * v)) / nu, restV: L / (nv - 1),
      mat: { front: base, back: base.map(x => x * 0.6), accent: PALETTE.robes[Math.floor(r() * 10)].map(x => x * 1.1), pattern: 2 + (r() < 0.5 ? 0 : 1), rough: 0.9 },
    }));
  }
  // Six banners hanging from rods on the tower's courtyard face.
  const heraldry = [
    { front: [0.42, 0.04, 0.03], accent: [0.80, 0.62, 0.20] }, { front: [0.06, 0.12, 0.34], accent: [0.85, 0.80, 0.70] },
    { front: [0.80, 0.70, 0.45], accent: [0.30, 0.03, 0.03] }, { front: [0.10, 0.22, 0.08], accent: [0.85, 0.70, 0.30] },
    { front: [0.40, 0.04, 0.03], accent: [0.80, 0.62, 0.20] }, { front: [0.06, 0.12, 0.34], accent: [0.85, 0.80, 0.70] },
  ];
  for (let b = 0; b < 6; b++) {
    const phi = (-52 + b * 20.8) * Math.PI / 180;
    const n = [Math.sin(phi), 0, -Math.cos(phi)], tg = [Math.cos(phi), 0, Math.sin(phi)];
    const rod = add([TOWER.x, 10.6, TOWER.z], mul(n, TOWER.r + 0.3));
    const nu = scaled(14, s), nv = scaled(28, s), W = 0.9, L = 2.7;
    sheets.push(sheet(KIND_BANNER, nu, nv, false, {
      char: -1, group: G_TOWER, rod, tg, n, length: L,
      world: (u, v) => add(add(rod, mul(tg, (u / (nu - 1) - 0.5) * W)), [0, -L * v / (nv - 1), 0]),
      pinned: (u, v) => v === 0,
      restU: () => W / (nu - 1), restV: L / (nv - 1),
      mat: { ...heraldry[b], back: heraldry[b].front.map(x => x * 0.7), pattern: 4 + (b % 2), rough: 0.85 },
    }));
  }
  // Four flags on poles above the battlements, pinned along the pole.
  for (let f = 0; f < 4; f++) {
    const phi = (-60 + f * 38) * Math.PI / 180;
    const n = [Math.sin(phi), 0, -Math.cos(phi)];
    const pole = add([TOWER.x, 0, TOWER.z], mul(n, TOWER.r - 0.15));
    const nu = scaled(24, s), nv = scaled(15, s), Lf = 1.6, Hf = 1.0;
    sheets.push(sheet(KIND_FLAG, nu, nv, false, {
      char: -1, group: G_NONE, pole, length: Lf,
      world: (u, v) => [pole[0] + Lf * u / (nu - 1) * 0.9, 17.2 - Hf * v / (nv - 1), pole[2] - Lf * u / (nu - 1) * 0.3],
      pinned: (u, v) => u === 0,
      restU: () => Lf / (nu - 1), restV: Hf / (nv - 1),
      mat: { ...heraldry[(f + 1) % 6], back: heraldry[(f + 1) % 6].front, pattern: 6, rough: 0.85 },
    }));
  }
  return sheets;
}

// ---- hair ----------------------------------------------------------------------------------------------

/** Root positions (head-local, relative to the head part's centre) and directions of the guides. */
export function hairRoots() {
  const out = [];
  const N = 900;
  const ga = Math.PI * (3 - Math.sqrt(5));
  for (let i = 0; i < N && out.length < HAIR.guides; i++) {
    const y = 1 - (i + 0.5) / N * 2, rr = Math.sqrt(1 - y * y), th = ga * i;
    const d = [rr * Math.cos(th), y, rr * Math.sin(th)];
    // Scalp: everything but the face and below the ears. z is forward.
    if (d[1] < -0.25) continue;
    if (d[2] > 0.25 && d[1] < 0.55) continue;
    if (d[2] > 0.55) continue;
    out.push(d);
  }
  // Fibonacci order runs top to bottom; keep the 128 that pass, spread over the scalp.
  return out.map(d => {
    const pos = mul(d, 0.098 + 0.03 * Math.abs(d[1]));
    const dir = norm(add(d, [0, -0.35, -0.75]));
    return { pos, dir };
  });
}

// ---- assembling the particle system --------------------------------------------------------------------

/**
 * Builds every static buffer of the particle system: particles (cloth first, then hair), constraints,
 * per-particle info, sim groups, pins, patches, triangles, materials, and hair children.
 */
export function buildSystem(cast, clothScale, strands, P) {
  const sheets = makeSheets(cast, clothScale);
  const pos = [], info = [], cons = [], uv = [], pins = [], groups = [];
  const addCons = [];
  let total = 0;

  // Pins: { char (−1 for world), local or world position }. Index into the pins buffer.
  const pin = p => { pins.push(p); return pins.length - 1; };
  const fr0 = cast.map(c => bodyFrame(c, 0));

  sheets.forEach((sh, si) => {
    sh.base = total;
    const { nu, nv } = sh;
    const idx = (u, v) => sh.base + v * nu + ((u % nu) + nu) % nu;
    for (let v = 0; v < nv; v++) for (let u = 0; u < nu; u++) {
      const uu = sh.wrap ? u / nu : u / (nu - 1), vv = v / (nv - 1);
      let w;
      if (sh.char >= 0) w = toWorld(fr0[sh.char], cast[sh.char].scale, sh.local(uu, vv));
      else w = sh.world(u, v);
      const pinned = sh.pinned(u, v);
      let pinIndex = NONE;
      if (pinned) pinIndex = sh.char >= 0 ? pin({ char: sh.char, local: sh.local(uu, vv) }) : pin({ char: -1, world: w });
      pos.push([...w, pinned ? 0 : 1]);
      uv.push([uu, vv, si, sh.kind]);
      const u0 = sh.wrap ? idx(u - 1, v) : idx(Math.max(u - 1, 0), v);
      const u1 = sh.wrap ? idx(u + 1, v) : idx(Math.min(u + 1, nu - 1), v);
      const v0 = idx(u, Math.max(v - 1, 0)), v1 = idx(u, Math.min(v + 1, nv - 1));
      info.push({ group: sh.group | (sh.kind << 16), pin: pinIndex, nb: [u0, u1, v0, v1] });
      // Constraints for this particle (each listed from both ends, Jacobi style).
      const list = [];
      const ru = sh.restU(vv), rv = sh.restV;
      const c = (du, dv, rest, k, kind = 0) => {
        let uu2 = u + du, vv2 = v + dv;
        if (vv2 < 0 || vv2 >= nv) return;
        if (!sh.wrap && (uu2 < 0 || uu2 >= nu)) return;
        const j = idx(uu2, vv2), i = idx(u, v);
        let jit = 1;
        if (sh.kind === KIND_CLOTHES && dv === 0) {
          const h = Math.imul(Math.min(i, j) ^ 0x9e3779b9, 0x85ebca6b) ^ Math.imul(Math.max(i, j), 0xc2b2ae35);
          jit = 1 + 0.12 * (((h >>> 8) & 0xffff) / 65535 - 0.5);
        }
        list.push([j, rest * jit, k, kind]);
      };
      for (const [du, dv] of [[1, 0], [-1, 0]]) c(du, dv, ru, 1);
      for (const [du, dv] of [[0, 1], [0, -1]]) c(du, dv, rv, 1);
      for (const [du, dv] of [[1, 1], [-1, 1], [1, -1], [-1, -1]]) c(du, dv, Math.hypot(ru, rv), 0.7);
      for (const [du, dv] of [[2, 0], [-2, 0]]) c(du, dv, 2 * ru, 0.25);
      for (const [du, dv] of [[0, 2], [0, -2]]) c(du, dv, 2 * rv, 0.25);
      // Long-range tether to the pinned particle of this column (or row, for flags).
      if (!pinned) {
        if (sh.kind === KIND_FLAG) list.push([idx(0, v), u * sh.restU(vv) * 1.01, 1, 1]);
        else {
          let geo = 0;
          for (let k = 0; k < v; k++) geo += rv;
          list.push([idx(u, 0), geo * 1.02, 1, 1]);
        }
      }
      addCons.push(list);
    }
    total += nu * nv;
    groups.push([sh.base, nu * nv, sh.kind, 0]);
  });
  const clothCount = total;

  // Hair guides on the hero's head.
  const roots = hairRoots();
  const G = HAIR.guides, S = HAIR.perGuide, seg = HAIR.length / (S - 1);
  const hairBase = total;
  const headLocal = k => roots[k];
  const headCentre = [0, 1.615, 0.011];
  roots.forEach((root, g) => {
    for (let k = 0; k < S; k++) {
      const local = k < 2
        ? add(add(headCentre, root.pos), mul(root.dir, seg * k))
        : add(add(add(headCentre, root.pos), mul(root.dir, seg)), [0, -seg * (k - 1) * 0.9, -Math.min(0.13, 0.045 * (k - 1))]);
      const w = toWorld(fr0[0], cast[0].scale, local);
      const pinned = k < 2;
      const pinIndex = pinned ? pin({ char: 0, local }) : NONE;
      pos.push([...w, pinned ? 0 : 1]);
      uv.push([g / G, k / (S - 1), -1, KIND_HAIR]);
      const i = hairBase + g * S + k;
      info.push({ group: 0 | (KIND_HAIR << 16), pin: pinIndex, nb: [k > 0 ? i - 1 : i, k < S - 1 ? i + 1 : i, hairBase + g * S, i] });
      const list = [];
      for (const d of [-1, 1]) if (k + d >= 0 && k + d < S) list.push([i + d, seg, 1, 0]);
      for (const d of [-2, 2]) if (k + d >= 0 && k + d < S) list.push([i + d, 2 * seg, 0.5, 0]);
      for (const d of [-3, 3]) if (k + d >= 0 && k + d < S) list.push([i + d, 3 * seg, 0.2, 0]);
      if (!pinned) list.push([hairBase + g * S, k * seg * 1.02, 1, 1]);
      addCons.push(list);
    }
  });
  total += G * S;
  for (let g = 0; g < G; g += 16) groups.push([hairBase + g * S, 16 * S, KIND_HAIR, 0]);
  const clothGroups = sheets.length;

  // Pack.
  const N = total;
  const pos0 = new Float32Array(N * 4), pinfo = new Uint32Array(N * 8), attr = new Float32Array(N * 8);
  let nc = 0;
  for (const l of addCons) nc += l.length;
  const consBuf = new ArrayBuffer(nc * 16), consF = new Float32Array(consBuf), consU = new Uint32Array(consBuf);
  let co = 0;
  for (let i = 0; i < N; i++) {
    pos0.set(pos[i], i * 4);
    const it = info[i];
    pinfo.set([co, addCons[i].length, it.group, it.pin, ...it.nb], i * 8);
    attr.set([0, 1, 0, 0, ...uv[i]], i * 8);
    for (const [j, rest, k, kind] of addCons[i]) {
      consU[co * 4] = j; consF[co * 4 + 1] = rest; consF[co * 4 + 2] = k; consF[co * 4 + 3] = kind;
      co++;
    }
  }

  // Patches: P×P quads of each sheet (common.wgsl unpacks this).
  const patches = [];
  sheets.forEach((sh, si) => {
    const qu = sh.wrap ? sh.nu : sh.nu - 1, qv = sh.nv - 1;
    sh.patch0 = patches.length;
    for (let v0 = 0; v0 < qv; v0 += P) for (let u0 = 0; u0 < qu; u0 += P) {
      patches.push([sh.base, sh.nu | (sh.wrap ? 1 << 16 : 0), u0 | (v0 << 16), Math.min(P, qu - u0) | (Math.min(P, qv - v0) << 8) | (si << 16)]);
    }
    sh.patchCount = patches.length - sh.patch0;
  });
  const patchInfo = new Uint32Array(patches.length * 4);
  patches.forEach((p, i) => patchInfo.set(p, i * 4));

  // Triangles for the raster path.
  const tris = [];
  for (const sh of sheets) {
    const qu = sh.wrap ? sh.nu : sh.nu - 1;
    const idx = (u, v) => sh.base + v * sh.nu + (u % sh.nu);
    for (let v = 0; v < sh.nv - 1; v++) for (let u = 0; u < qu; u++) {
      const a = idx(u, v), b = idx(u + 1, v), c = idx(u, v + 1), d = idx(u + 1, v + 1);
      tris.push(a, c, b, b, c, d);
    }
  }

  // Materials: 4 vec4 per sheet (common.wgsl M_*).
  const mats = new Float32Array(MAX_SHEETS * 16);
  sheets.forEach((sh, si) => {
    const m = sh.mat;
    mats.set([...m.front, m.pattern, ...m.back, m.rough, ...m.accent, sh.kind, sh.nu, sh.nv, sh.wrap ? 1 : 0, 0], si * 16);
  });

  // Hair children: each render strand follows guide g, blended toward a neighbour guide g2, offset into a clump.
  const nbr = roots.map((a, g) => roots.map((b, h) => [h, len(sub(a.pos, b.pos))]).filter(([h]) => h !== g)
    .sort((x, y) => x[1] - y[1]).slice(0, 4).map(([h]) => h));
  const rc = rng(99), children = new ArrayBuffer(strands * 32);
  const chF = new Float32Array(children), chU = new Uint32Array(children);
  for (let c = 0; c < strands; c++) {
    const g = c % G;
    const g2 = nbr[g][Math.floor(rc() * 4)];
    const rr = Math.sqrt(rc()), th = 2 * Math.PI * rc();
    chU[c * 8] = g; chU[c * 8 + 1] = g2;
    chF[c * 8 + 2] = 0.5 * rc() ** 1.5;                 // blend toward g2
    chF[c * 8 + 3] = rc();                               // colour variation
    chF[c * 8 + 4] = rr * Math.cos(th); chF[c * 8 + 5] = rr * Math.sin(th);
    chF[c * 8 + 6] = rc() * 6.283;                       // frizz phase
    chF[c * 8 + 7] = 0.85 + 0.3 * rc();                  // length variation
  }

  return {
    sheets, cast, N, clothCount, hairBase, hairCount: G * S, pins, groups, clothGroups,
    pos0, pinfo, attr, cons: new Uint32Array(consBuf), nCons: nc,
    patchInfo, nPatches: patches.length, P,
    tris: new Uint32Array(tris), mats, children: new Uint32Array(children), strands,
    strandRadius: HAIR.radius2048 * 2048 / strands,
  };
}

/** Pin targets at time t (world xyz per pin). */
export function writePins(sys, frames, out, off) {
  sys.pins.forEach((p, i) => {
    const w = p.char >= 0 ? toWorld(frames[p.char], sys.cast[p.char].scale, p.local) : p.world;
    out.set(w, off + i * 4);
  });
}

// ---- cameras -----------------------------------------------------------------------------------------

export const VIEWS = {
  courtyard: { eye: [-2.6, 1.6, -6.2], target: [1.4, 4.3, 8.0], fovy: 55, extent: 17, centre: [1.5, 0, 6.5] },
  hero: { eye: [-1.45, 1.62, -3.95], target: [-0.32, 1.22, -1.95], fovy: 42, extent: 7, centre: [0.5, 0, 1.5] },
  banners: { eye: [-4.8, 1.6, 10.4], target: [2.4, 9.6, 13.2], fovy: 52, extent: 12, centre: [2.0, 0, 11.5] },
};
