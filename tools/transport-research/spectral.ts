/** Experimental shade-then-integrate lowering for two affine authored phases.
 * Truncation bounds apply to the finite Fourier interpolant, not the source BSDF. */
export type Pair = [number, number];
export interface Footprint {
  dx: Pair;
  dy: Pair;
  shutter: Pair;
}
export interface Mode {
  m: number;
  n: number;
  re: number;
  im: number;
}
export interface Spectrum {
  size: number;
  dc: number;
  modes: Mode[];
  discardedNyquistL1: number;
  samples: Float64Array;
}
export interface Plan {
  dc: number;
  modes: Mode[];
  omittedL1: number;
  footprint: Footprint;
}
export const tau = 2 * Math.PI;
export function sinc(x: number) {
  return Math.abs(x) < 1e-5 ? 1 - (x * x) / 6 + x ** 4 / 120 : Math.sin(x) / x;
}
export function transfer(m: number, n: number, footprint: Footprint) {
  return (
    sinc((m * footprint.dx[0] + n * footprint.dx[1]) / 2) *
    sinc((m * footprint.dy[0] + n * footprint.dy[1]) / 2) *
    sinc((m * footprint.shutter[0] + n * footprint.shutter[1]) / 2)
  );
}
export function fft(re: Float64Array, im: Float64Array) {
  const size = re.length;
  for (let i = 1, j = 0; i < size; i++) {
    let bit = size >> 1;
    while (j & bit) {
      j ^= bit;
      bit >>= 1;
    }
    j ^= bit;
    if (i < j) {
      [re[i], re[j]] = [re[j], re[i]];
      [im[i], im[j]] = [im[j], im[i]];
    }
  }
  for (let width = 2; width <= size; width *= 2) {
    const c = Math.cos(-tau / width),
      s = Math.sin(-tau / width);
    for (let base = 0; base < size; base += width) {
      let wr = 1,
        wi = 0;
      for (let j = 0; j < width / 2; j++) {
        const a = base + j,
          b = a + width / 2;
        const tr = wr * re[b] - wi * im[b],
          ti = wr * im[b] + wi * re[b];
        re[b] = re[a] - tr;
        im[b] = im[a] - ti;
        re[a] += tr;
        im[a] += ti;
        [wr, wi] = [wr * c - wi * s, wr * s + wi * c];
      }
    }
  }
}
export function compileSpectrum(response: (a: number, b: number) => number, size = 256): Spectrum {
  if (size < 4 || size & (size - 1)) throw new Error("FFT size must be a power of two >= 4");
  const samples = new Float64Array(size * size),
    re = new Float64Array(size * size);
  const im = new Float64Array(size * size),
    rowRe = new Float64Array(size),
    rowIm = new Float64Array(size);
  for (let y = 0; y < size; y++) {
    rowIm.fill(0);
    for (let x = 0; x < size; x++)
      samples[y * size + x] = rowRe[x] = response((tau * x) / size, (tau * y) / size);
    fft(rowRe, rowIm);
    re.set(rowRe, y * size);
    im.set(rowIm, y * size);
  }
  for (let x = 0; x < size; x++) {
    for (let y = 0; y < size; y++) {
      rowRe[y] = re[y * size + x];
      rowIm[y] = im[y * size + x];
    }
    fft(rowRe, rowIm);
    for (let y = 0; y < size; y++) {
      re[y * size + x] = rowRe[y] / (size * size);
      im[y * size + x] = rowIm[y] / (size * size);
    }
  }
  const modes: Mode[] = [];
  let discardedNyquistL1 = 0;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const i = y * size + x;
      if (x === size / 2 || y === size / 2) {
        discardedNyquistL1 += Math.hypot(re[i], im[i]);
        continue;
      }
      const m = x < size / 2 ? x : x - size,
        n = y < size / 2 ? y : y - size;
      if (m > 0 || (m === 0 && n > 0)) modes.push({ m, n, re: re[i], im: im[i] });
    }
  }
  return { size, dc: re[0], modes, discardedNyquistL1, samples };
}
export function planSpectrum(
  spectrum: Spectrum,
  footprint: Footprint,
  maxModes = 64,
  absoluteBudget = 0,
): Plan {
  const weighted = spectrum.modes
    .map((mode) => {
      const attenuation = transfer(mode.m, mode.n, footprint);
      return { ...mode, re: mode.re * attenuation, im: mode.im * attenuation };
    })
    .sort((a, b) => Math.hypot(b.re, b.im) - Math.hypot(a.re, a.im));
  let omittedL1 = weighted.reduce((s, m) => s + 2 * Math.hypot(m.re, m.im), 0);
  const modes: Mode[] = [];
  for (const mode of weighted) {
    if (modes.length >= maxModes || omittedL1 <= absoluteBudget) break;
    modes.push(mode);
    omittedL1 = Math.max(0, omittedL1 - 2 * Math.hypot(mode.re, mode.im));
  }
  return { dc: spectrum.dc, modes, omittedL1, footprint };
}
export function evaluatePlan(plan: Pick<Plan, "dc" | "modes">, a: number, b: number) {
  let sum = plan.dc;
  for (const mode of plan.modes) {
    const phase = mode.m * a + mode.n * b;
    sum += 2 * (mode.re * Math.cos(phase) - mode.im * Math.sin(phase));
  }
  return sum;
}
export function integrateResponse(
  response: (a: number, b: number) => number,
  a: number,
  b: number,
  footprint: Footprint,
  side = 64,
  times = 1,
) {
  let result = 0;
  for (let t = 0; t < times; t++)
    for (let y = 0; y < side; y++)
      for (let x = 0; x < side; x++) {
        const u = (x + 0.5) / side - 0.5,
          v = (y + 0.5) / side - 0.5,
          s = (t + 0.5) / times - 0.5;
        result += response(
          a + footprint.dx[0] * u + footprint.dy[0] * v + footprint.shutter[0] * s,
          b + footprint.dx[1] * u + footprint.dy[1] * v + footprint.shutter[1] * s,
        );
      }
  return result / (side * side * times);
}
export function footprintReuseBound(plan: Pick<Plan, "modes">, from: Footprint, to: Footprint) {
  let bound = 0;
  // Use unfiltered coefficients here. E|u| = 1/4 for the unit box coordinate.
  for (const mode of plan.modes) {
    let delta = 0;
    for (const key of ["dx", "dy", "shutter"] as const) {
      delta += Math.abs(mode.m * (to[key][0] - from[key][0]) + mode.n * (to[key][1] - from[key][1]));
    }
    bound += 2 * Math.hypot(mode.re, mode.im) * Math.min(2, delta / 4);
  }
  return bound;
}
const normalized = (v: number[]) => {
  const l = Math.hypot(...v);
  return v.map((x) => x / l);
};
export const water = {
  // The authored reference project's two waves, reduced to their slope vectors.
  slopes: [
    [((0.12 * tau) / 8) * Math.cos(0.4), ((0.12 * tau) / 8) * Math.sin(0.4)],
    [((0.06 * tau) / 3.2) * Math.cos(1.7), ((0.06 * tau) / 3.2) * Math.sin(1.7)],
  ],
  light: normalized([0.32, 0.86, 0.4]),
  view: normalized([-0.36, 0.83, -0.43]),
  roughness: 0.18,
  f0: 0.02037,
};
/** GGX/Smith/Schlick direct specular radiance for unit incident radiance.
 * This is single scattering, opaque reflection only, with fixed light/view. */
export function waterResponse(a: number, b: number, attenuation: Pair = [1, 1]) {
  const ca = Math.cos(a) * attenuation[0],
    cb = Math.cos(b) * attenuation[1];
  const sx = water.slopes[0][0] * ca + water.slopes[1][0] * cb;
  const sz = water.slopes[0][1] * ca + water.slopes[1][1] * cb;
  return microfacetResponse(sx, sz, water.roughness, water.f0);
}
export function microfacetResponse(sx: number, sz: number, roughness: number, f0: number, diffuse = 0) {
  const length = Math.hypot(sx, 1, sz),
    n = [-sx / length, 1 / length, -sz / length];
  const dot = (u: number[], v: number[]) => u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
  const h = normalized(water.light.map((x, i) => x + water.view[i]));
  const nv = Math.max(0, dot(n, water.view)),
    nl = Math.max(0, dot(n, water.light));
  if (nl <= 0 || nv <= 0) return 0;
  const nh = Math.max(0, dot(n, h)),
    vh = Math.max(0, dot(water.view, h));
  const alpha2 = roughness ** 4,
    d = nh * nh * (alpha2 - 1) + 1;
  const distribution = alpha2 / (Math.PI * d * d);
  const lambda = (c: number) => (Math.sqrt(1 + (alpha2 * (1 - c * c)) / (c * c)) - 1) / 2;
  const g = 1 / (1 + lambda(nv) + lambda(nl));
  const f = f0 + (1 - f0) * (1 - vh) ** 5;
  return (distribution * g * f) / (4 * nv) + (diffuse * (1 - f) * nl) / Math.PI;
}
/** Correlated procedural metal/roughness/albedo, in the same two authored phases.
 * Filtering each input independently is intentionally a control, not the compiler. */
export function materialResponse(a: number, b: number, footprint?: Footprint) {
  const filter = (m: number, n: number) => (footprint ? transfer(m, n, footprint) : 1);
  const ca = Math.cos(a) * filter(1, 0),
    cb = Math.cos(b) * filter(0, 1);
  const sx = water.slopes[0][0] * ca + water.slopes[1][0] * cb;
  const sz = water.slopes[0][1] * ca + water.slopes[1][1] * cb;
  const roughness = 0.27 + 0.11 * Math.sin(a + 2 * b) * filter(1, 2);
  const metallic = 0.5 + 0.4 * Math.cos(a - b) * filter(1, -1);
  const base = 0.4 + 0.2 * Math.cos(b) * filter(0, 1);
  return microfacetResponse(
    sx,
    sz,
    roughness,
    0.04 * (1 - metallic) + base * metallic,
    base * (1 - metallic),
  );
}
