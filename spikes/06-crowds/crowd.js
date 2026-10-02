// The crowd: per-instance parameters by seed, the per-frame root state (the "sim"), and a JS copy
// of pose.wgsl (`poseJS`) for timing posing on the CPU and checking the GPU pass against.
//
// Instance i's parameters depend only on i, so a crowd of 300 is the first 300 of the 1,000.

export const T_VILLAGER = 0, T_ANIMAL = 1, T_BIRD = 2;
export const STRIDE = 36;        // vec4s per posed instance (common.wgsl)
export const S_FLOATS = 8;       // root state per instance: 2 vec4s
export const P_FLOATS = 16;      // static parameters per instance: 4 vec4s
const I_PARTS = 4;

/** mulberry32, as spike 01's grazer.js. */
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

const smoothstep = (a, b, x) => { const t = Math.min(Math.max((x - a) / (b - a), 0), 1); return t * t * (3 - 2 * t); };
const PLAZA = [-18, -14, 18, 18];

function rectSd(x, z, r) {
  const cx = (r[0] + r[2]) / 2, cz = (r[1] + r[3]) / 2, hx = (r[2] - r[0]) / 2, hz = (r[3] - r[1]) / 2;
  const qx = Math.abs(x - cx) - hx, qz = Math.abs(z - cz) - hz;
  return Math.hypot(Math.max(qx, 0), Math.max(qz, 0)) + Math.min(Math.max(qx, qz), 0);
}

/** Must match `terrain_h` in common.wgsl. */
export function terrainHeight(x, z) {
  let h = 0.9 * Math.sin(0.061 * x + 0.4) * Math.cos(0.047 * z + 1.1)
        + 0.45 * Math.sin(0.13 * x + 1.7) * Math.sin(0.11 * z + 0.3)
        + 0.12 * Math.sin(0.37 * x) * Math.cos(0.29 * z + 0.7);
  const r = Math.hypot(x, z);
  if (r > 60) {
    const a = Math.atan2(z, x);
    h += 30 * smoothstep(60, 300, r) * (0.62 + 0.38 * Math.sin(3 * a + 0.5) * Math.cos(0.013 * r));
  }
  const m = 1 - smoothstep(0, 8, rectSd(x, z, PLAZA));
  return h + (0.05 + 0.004 * z - h) * m;
}

const pick = (r, a) => a[Math.floor(r() * a.length) % a.length];
const CLOTHES = [[0.30, 0.06, 0.04], [0.05, 0.09, 0.25], [0.07, 0.14, 0.05], [0.22, 0.15, 0.08], [0.10, 0.07, 0.05],
  [0.35, 0.30, 0.22], [0.18, 0.05, 0.12], [0.40, 0.28, 0.05], [0.12, 0.12, 0.13]];

/** One instance's parameters and motion, from its index alone. */
function makeOne(i) {
  const r = rng(1000 + i * 7919);
  const type = i % 10 === 9 ? T_BIRD : (r() < 0.6 ? T_VILLAGER : T_ANIMAL);
  const p = new Float32Array(P_FLOATS);
  const m = { type };
  if (type === T_VILLAGER) {
    const H = 1.55 + 0.3 * r(), w = 0.9 + 0.25 * r();
    const robe = r() < 0.35, hat = r() < 0.3;
    const en = 0x0FFF | (robe ? 1 << 12 : 0) | (hat ? 1 << 13 : 0);
    const skin = 0.6 + 0.5 * r();
    p.set([T_VILLAGER, en, (1 << 1) | (1 << 9) | (1 << 11), 0.045]);
    p.set([H, w, 0.3 + 0.2 * r(), 0.35 + 0.15 * r()], 4);
    p.set([...pick(r, CLOTHES), 0], 8);
    p.set([0.45 * skin, 0.28 * skin, 0.18 * skin, 0], 12);
    // Wander: circles in the square (some on the road south), a few standing.
    const road = r() < 0.15;
    m.cx = road ? 3 + 4 * (r() - 0.5) : -15 + 30 * r();
    m.cz = road ? -18 - 14 * r() : -11 + 26 * r();
    m.R = 1.5 + 5.5 * r();
    m.speed = r() < 0.15 ? 0 : 0.9 + 0.6 * r();
    m.stride = 0.82 * H;
  } else if (type === T_ANIMAL) {
    const k = r(), s = 0.9 + 0.2 * r();
    let shape, a, b, second = 0, down = r() < 0.35 ? 1 : 0;
    if (k < 0.35) {        // sheep
      shape = [0.7, 0.36, 0.33, 0.22, 0.24]; a = [0.55, 0.52, 0.45]; b = [0.05, 0.045, 0.04];
      second = (1 << 2) | (1 << 4) | (1 << 6) | (1 << 8) | (1 << 10);
    } else if (k < 0.5) {  // pig
      shape = [0.75, 0.22, 0.27, 0.12, 0.25]; a = [0.55, 0.30, 0.25]; b = [0.45, 0.24, 0.2];
    } else if (k < 0.7) {  // cow
      shape = [1.35, 0.7, 0.42, 0.45, 0.42];
      a = r() < 0.5 ? [0.25, 0.12, 0.06] : [0.5, 0.48, 0.45]; b = [0.04, 0.03, 0.025];
      second = (1 << 4) | (1 << 6) | (1 << 8) | (1 << 10);
    } else if (k < 0.85) { // horse
      shape = [1.2, 0.85, 0.36, 0.65, 0.5]; a = [0.20, 0.08, 0.035]; b = [0.02, 0.018, 0.016];
      second = (1 << 4) | (1 << 6) | (1 << 8) | (1 << 10) | (1 << 11); down = 0;
    } else {               // dog
      shape = [0.55, 0.3, 0.15, 0.18, 0.2]; a = r() < 0.5 ? [0.25, 0.15, 0.07] : [0.05, 0.045, 0.04]; b = [0.4, 0.35, 0.3];
      second = 1 << 2; down = 0;
    }
    const sh = shape.map(x => x * s);
    p.set([T_ANIMAL, 0x0FFF, second, 0.03 + 0.08 * sh[2]]);
    p.set(sh.slice(0, 4), 4);
    p.set([...a, sh[4]], 8);
    p.set([...b, down], 12);
    const dog = k >= 0.85;
    m.cx = dog ? -15 + 30 * r() : 4 + 18 * r();
    m.cz = dog ? -11 + 26 * r() : -44 + 22 * r();
    m.R = 1 + 4 * r();
    m.speed = down ? 0 : (dog ? 1.2 + 0.6 * r() : 0.4 + 0.5 * r());
    m.stride = 1.4 * sh[1] + 0.4;
  } else {
    const span = 0.8 + 0.8 * r();
    const gull = r() < 0.4;
    p.set([T_BIRD, 0xFF, 1 << 2, 0.015 * span]);
    p.set([span, 0.55 + 0.25 * r(), 0, 0], 4);
    p.set([...(gull ? [0.6, 0.6, 0.58] : [0.03, 0.03, 0.035]), 0], 8);
    p.set([0.4, 0.25, 0.05, 0], 12);
    m.cx = -15 + 30 * r();
    m.cz = -10 + 30 * r();
    m.R = 8 + 20 * r();
    m.alt = 12 + 18 * r();
    m.speed = 6 + 5 * r();
    m.flap = 2.5 + 1.5 * r();
  }
  m.theta0 = 2 * Math.PI * r();
  m.dir = r() < 0.5 ? -1 : 1;
  m.phase0 = r();
  return { params: p, m };
}

export function makeCrowd(n) {
  const list = [];
  for (let i = 0; i < n; i++) list.push(makeOne(i));
  const params = new Float32Array(n * P_FLOATS);
  list.forEach((c, i) => params.set(c.params, i * P_FLOATS));
  return { n, list, params };
}

/** The per-frame root state, what a sim would hand the renderer: position, heading, gait phase,
 *  bank. 32 bytes per instance. Closed-form circles, so any frame can be reproduced exactly. */
export function writeStates(crowd, n, t, out) {
  for (let i = 0; i < n; i++) {
    const m = crowd.list[i].m, o = i * S_FLOATS;
    const w = m.dir * m.speed / m.R;
    const th = m.theta0 + w * t;
    const x = m.cx + m.R * Math.cos(th), z = m.cz + m.R * Math.sin(th);
    if (m.type === T_BIRD) {
      const vx = -Math.sin(th) * m.dir, vz = Math.cos(th) * m.dir;
      const bank = m.dir * Math.atan(m.speed * m.speed / (9.8 * m.R));
      const glide = smoothstep(0.3, 0.7, 0.5 + 0.5 * Math.sin(0.35 * t + 6.28 * m.phase0));
      out[o] = x; out[o + 1] = m.alt + 1.5 * Math.sin(0.5 * t + 6.28 * m.phase0); out[o + 2] = z; out[o + 3] = Math.atan2(vx, vz);
      out[o + 4] = (m.phase0 + m.flap * t) % 1; out[o + 5] = bank; out[o + 6] = glide; out[o + 7] = 0;
    } else if (m.speed === 0) {
      out[o] = m.cx + m.R * Math.cos(m.theta0); out[o + 1] = 0; out[o + 2] = m.cz + m.R * Math.sin(m.theta0); out[o + 3] = m.theta0 + 1;
      out[o + 4] = 0; out[o + 5] = 0; out[o + 6] = 0; out[o + 7] = 0;
    } else {
      const vx = -Math.sin(th) * m.dir, vz = Math.cos(th) * m.dir;
      out[o] = x; out[o + 1] = 0; out[o + 2] = z; out[o + 3] = Math.atan2(vx, vz);
      out[o + 4] = (m.phase0 + m.speed * t / m.stride) % 1; out[o + 5] = 0; out[o + 6] = 0; out[o + 7] = 0;
    }
  }
}

// ---- poseJS: pose.wgsl, line for line --------------------------------------------------------------

/** Poses instances [0, n) from `states` into `out` (STRIDE vec4s each), as the GPU pass does. */
export function poseJS(crowd, states, n, out) {
  const P = crowd.params;
  let ax0, ax1, ax2, ay0, ay1, ay2, az0, az2, rx, ry, rz, base, K;
  let lo0, lo1, lo2, hi0, hi1, hi2;
  const put = (i, a, r1, b, r2) => {
    const wa0 = rx + ax0 * a[0] + ay0 * a[1] + az0 * a[2], wa1 = ry + ax1 * a[0] + ay1 * a[1], wa2 = rz + ax2 * a[0] + ay2 * a[1] + az2 * a[2];
    const wb0 = rx + ax0 * b[0] + ay0 * b[1] + az0 * b[2], wb1 = ry + ax1 * b[0] + ay1 * b[1], wb2 = rz + ax2 * b[0] + ay2 * b[1] + az2 * b[2];
    const o = (base + I_PARTS + 2 * i) * 4;
    out[o] = wa0; out[o + 1] = wa1; out[o + 2] = wa2; out[o + 3] = r1;
    out[o + 4] = wb0; out[o + 5] = wb1; out[o + 6] = wb2; out[o + 7] = r2;
    const r = Math.max(r1, r2) + K;
    lo0 = Math.min(lo0, wa0 - r, wb0 - r); lo1 = Math.min(lo1, wa1 - r, wb1 - r); lo2 = Math.min(lo2, wa2 - r, wb2 - r);
    hi0 = Math.max(hi0, wa0 + r, wb0 + r); hi1 = Math.max(hi1, wa1 + r, wb1 + r); hi2 = Math.max(hi2, wa2 + r, wb2 + r);
  };
  const limb = (o, ang, len) => [o[0], o[1] - len * Math.cos(ang), o[2] + len * Math.sin(ang)];
  const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];

  for (let i = 0; i < n; i++) {
    const so = i * S_FLOATS, po = i * P_FLOATS;
    const ty = P[po];
    K = P[po + 3];
    base = i * STRIDE;
    const y = ty !== T_BIRD ? terrainHeight(states[so], states[so + 2]) : states[so + 1];
    rx = states[so]; ry = y; rz = states[so + 2];
    const cy = Math.cos(states[so + 3]), sy = Math.sin(states[so + 3]);
    const cb = Math.cos(states[so + 5]), sb = Math.sin(states[so + 5]);
    ax0 = cy * cb; ax1 = sb; ax2 = -sy * cb;
    ay0 = -cy * sb; ay1 = cb; ay2 = sy * sb;
    az0 = sy; az2 = cy;
    lo0 = lo1 = lo2 = Infinity; hi0 = hi1 = hi2 = -Infinity;
    const ph = 2 * Math.PI * states[so + 4];
    const p1 = [P[po + 4], P[po + 5], P[po + 6], P[po + 7]];
    if (ty === T_VILLAGER) {
      const H = p1[0], w = p1[1], s = Math.sin(ph), c = Math.cos(ph), en = P[po + 1];
      const bob = 0.012 * H * Math.cos(2 * ph), hipY = 0.5 * H + bob;
      for (let side = 0; side < 2; side++) {
        const sg = side === 1 ? 1 : -1;
        const alpha = sg * p1[3] * s;
        const knee = 0.1 + 0.7 * Math.max(0, sg * c);
        const hip = [sg * 0.085 * H * w, hipY, 0];
        const kn = limb(hip, alpha, 0.245 * H);
        const an = limb(kn, alpha - knee, 0.235 * H);
        const toe = add(an, [0, -0.012 * H, 0.07 * H]);
        const i0 = 2 + 3 * side;
        put(i0, hip, 0.05 * H * w, kn, 0.038 * H);
        put(i0 + 1, kn, 0.036 * H, an, 0.026 * H);
        put(i0 + 2, an, 0.024 * H, toe, 0.02 * H);
        const beta = -sg * p1[2] * s;
        const sh = [sg * 0.118 * H * w, 0.765 * H + bob, 0];
        const el = add(limb(sh, beta, 0.17 * H), [sg * 0.012 * H, 0, 0]);
        const ha = limb(el, beta + 0.35, 0.16 * H);
        put(8 + 2 * side, sh, 0.034 * H * w, el, 0.028 * H);
        put(9 + 2 * side, el, 0.027 * H, ha, 0.022 * H);
      }
      put(0, [0, hipY + 0.03 * H, 0], 0.09 * H * w, [0, 0.75 * H + bob, 0.01 * H], 0.105 * H * w);
      const hc = [0, 0.9 * H + bob, 0.015 * H];
      put(1, add(hc, [0, -0.02 * H, 0]), 0.058 * H, add(hc, [0, 0.012 * H, 0.004 * H]), 0.062 * H);
      if (en & (1 << 12)) put(12, [0, 0.56 * H + bob, 0], 0.085 * H * w, [0, 0.1 * H, 0], 0.15 * H * w);
      if (en & (1 << 13)) put(13, add(hc, [0, 0.045 * H, 0]), 0.095 * H, add(hc, [0, 0.1 * H, 0]), 0.05 * H);
    } else if (ty === T_ANIMAL) {
      const [L, legL, rb, neckL] = p1, hs = P[po + 11], down = P[po + 15];
      const bob = 0.015 * legL * Math.cos(2 * ph), by = legL + 0.35 * rb + bob;
      put(0, [0, by, -0.5 * L], 0.88 * rb, [0, by + 0.04 * rb, 0.5 * L], rb);
      const na = 0.75 + (-0.8 - 0.75) * down + 0.06 * Math.sin(2 * ph);
      const n0 = [0, by + 0.3 * rb, 0.5 * L + 0.2 * rb];
      const n1 = add(n0, [0, neckL * Math.sin(na), neckL * Math.cos(na)]);
      put(1, n0, 0.5 * rb, n1, 0.32 * rb);
      const ha = na - 0.9 + 0.5 * down;
      put(2, n1, 0.34 * rb, add(n1, [0, hs * Math.sin(ha), hs * Math.cos(ha)]), 0.2 * rb);
      const reach = by - 0.2 * rb;
      for (let l = 0; l < 4; l++) {
        const front = l < 2, sg = (l & 1) === 1 ? 1 : -1;
        const lp = ph + (l === 1 || l === 2 ? Math.PI : 0);
        const alpha = 0.36 * Math.sin(lp), flex = 0.55 * Math.max(0, Math.cos(lp));
        const j = [sg * 0.55 * rb, reach, front ? 0.5 * L - 0.1 * rb : -0.5 * L + 0.15 * rb];
        const kn = limb(j, alpha, 0.5 * reach);
        const ft = limb(kn, alpha - flex, 0.5 * reach);
        put(3 + 2 * l, j, 0.06 + 0.13 * rb, kn, 0.04 + 0.07 * rb);
        put(4 + 2 * l, kn, 0.035 + 0.06 * rb, ft, 0.03 + 0.05 * rb);
      }
      const t0 = [0, by + 0.25 * rb, -0.5 * L - 0.75 * rb];
      const sway = 0.25 * Math.sin(2 * ph + 1);
      put(11, t0, 0.07 * rb + 0.02, add(t0, [0.35 * L * sway, -0.35 * L * 0.75, -0.35 * L * 0.45]), 0.04 * rb + 0.012);
    } else {
      const s = p1[0], glide = states[so + 6];
      const th = p1[1] * (1 - glide) * Math.sin(ph) + 0.12;
      put(0, [0, 0, -0.16 * s], 0.035 * s, [0, 0.01 * s, 0.12 * s], 0.06 * s);
      put(1, [0, 0.03 * s, 0.19 * s], 0.042 * s, [0, 0.04 * s, 0.22 * s], 0.04 * s);
      put(2, [0, 0.035 * s, 0.25 * s], 0.016 * s, [0, 0.028 * s, 0.31 * s], 0.004 * s);
      for (let side = 0; side < 2; side++) {
        const sg = side === 1 ? 1 : -1;
        const sh = [sg * 0.04 * s, 0.025 * s, 0.05 * s];
        const el = add(sh, [0.27 * s * sg * Math.cos(th), 0.27 * s * Math.sin(th), 0.27 * s * -0.03]);
        const th2 = 1.35 * th - 0.04;
        const tip = add(el, [0.27 * s * sg * Math.cos(th2), 0.27 * s * Math.sin(th2), 0.27 * s * -0.14]);
        put(3 + 2 * side, sh, 0.04 * s, el, 0.028 * s);
        put(4 + 2 * side, el, 0.026 * s, tip, 0.006 * s);
      }
      put(7, [0, 0, -0.14 * s], 0.02 * s, [0, 0.005 * s, -0.34 * s], 0.05 * s);
    }
    const o = base * 4;
    out[o] = K; out[o + 1] = ty; out[o + 2] = P[po + 1]; out[o + 3] = P[po + 2];
    out[o + 4] = P[po + 8]; out[o + 5] = P[po + 9]; out[o + 6] = P[po + 10]; out[o + 7] = 0;
    out[o + 8] = P[po + 12]; out[o + 9] = P[po + 13]; out[o + 10] = P[po + 14]; out[o + 11] = 0;
    out[o + 12] = (lo0 + hi0) / 2; out[o + 13] = (lo1 + hi1) / 2; out[o + 14] = (lo2 + hi2) / 2;
    out[o + 15] = 0.5 * Math.hypot(hi0 - lo0, hi1 - lo1, hi2 - lo2);
  }
}
