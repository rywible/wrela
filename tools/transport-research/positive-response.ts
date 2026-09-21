/** Positive transport algebra: filter squared amplitudes through a PSD Gram matrix.
 * General harmonic analysis, specialized here to the authored phase program. */
import {
  compileSpectrum,
  evaluatePlan,
  type Footprint,
  type Mode,
  type Plan,
  planSpectrum,
  transfer,
} from "./spectral";

export function signedModes(plan: Pick<Plan, "dc" | "modes">): Mode[] {
  return [
    { m: 0, n: 0, re: plan.dc, im: 0 },
    ...plan.modes.flatMap((mode) => [mode, { m: -mode.m, n: -mode.n, re: mode.re, im: -mode.im }]),
  ];
}
export function compilePositive(response: (a: number, b: number) => number, modes = 64, size = 256) {
  const spectrum = compileSpectrum((a, b) => {
    const value = response(a, b);
    if (value < 0) throw new Error("Physical radiance must be nonnegative before square-root factorization");
    return Math.sqrt(value);
  }, size);
  const amplitude = planSpectrum(spectrum, { dx: [0, 0], dy: [0, 0], shutter: [0, 0] }, modes);
  return { amplitude, signed: signedModes(amplitude), discardedNyquistL1: spectrum.discardedNyquistL1 };
}
export function positiveIntegral(modes: Mode[], a: number, b: number, footprint: Footprint) {
  const phase = modes.map((mode) => {
    const p = mode.m * a + mode.n * b,
      c = Math.cos(p),
      s = Math.sin(p);
    return [mode.re * c - mode.im * s, mode.re * s + mode.im * c];
  });
  let sum = 0;
  for (let i = 0; i < modes.length; i++) {
    sum += phase[i][0] ** 2 + phase[i][1] ** 2;
    for (let j = 0; j < i; j++)
      sum +=
        2 *
        (phase[i][0] * phase[j][0] + phase[i][1] * phase[j][1]) *
        transfer(modes[i].m - modes[j].m, modes[i].n - modes[j].n, footprint);
  }
  // Do not clamp: cancellation error and incorrect Gram approximations must stay observable.
  return sum;
}
/** Exact full-period orbit average when dx = 2*pi*N*winding, N nonzero integer,
 * dy=shutter=0. The caller must establish this domain condition. */
export function orbitIntegral(modes: Mode[], a: number, b: number, winding: [number, number]) {
  if (!winding.every(Number.isInteger) || winding.every((x) => x === 0))
    throw new Error("Nonzero integer phase winding required");
  const classes = new Map<number, [number, number]>();
  for (const mode of modes) {
    const key = mode.m * winding[0] + mode.n * winding[1],
      sum = classes.get(key) ?? [0, 0];
    const p = mode.m * a + mode.n * b,
      c = Math.cos(p),
      s = Math.sin(p);
    sum[0] += mode.re * c - mode.im * s;
    sum[1] += mode.re * s + mode.im * c;
    classes.set(key, sum);
  }
  return [...classes.values()].reduce((sum, [re, im]) => sum + re * re + im * im, 0);
}
/** Exact signed-frequency convolution for correlated illumination and material.
 * The result is real if inputs have conjugate symmetry. */
export function multiplyPrograms(a: Mode[], b: Mode[]): Mode[] {
  const result = new Map<string, Mode>();
  for (const x of a)
    for (const y of b) {
      const m = x.m + y.m,
        n = x.n + y.n,
        key = `${m},${n}`;
      const old = result.get(key) ?? { m, n, re: 0, im: 0 };
      old.re += x.re * y.re - x.im * y.im;
      old.im += x.re * y.im + x.im * y.re;
      result.set(key, old);
    }
  return [...result.values()];
}
export function integralSigned(modes: Mode[], a: number, b: number, footprint: Footprint) {
  let sum = 0;
  for (const mode of modes) {
    const p = mode.m * a + mode.n * b;
    sum += (mode.re * Math.cos(p) - mode.im * Math.sin(p)) * transfer(mode.m, mode.n, footprint);
  }
  return sum;
}
/** Uniformly sampled residual correction. The predictor must be fixed independently
 * of these samples, or trained on an independent fold. No radiance clipping. */
export function residualSample(
  response: (a: number, b: number) => number,
  predictor: Pick<Plan, "dc" | "modes">,
  a: number,
  b: number,
  footprint: Footprint,
  u: number,
  v: number,
  time = 0,
) {
  const x = a + footprint.dx[0] * u + footprint.dy[0] * v + footprint.shutter[0] * time;
  const y = b + footprint.dx[1] * u + footprint.dy[1] * v + footprint.shutter[1] * time;
  return response(x, y) - evaluatePlan(predictor, x, y);
}
