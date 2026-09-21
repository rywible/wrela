/** A view/light-independent NDF candidate for fully decorrelated authored waves.
 * Microgeometry uses additive slopes with GGX slope density. This is not an exact
 * replacement for averaging the full BRDF, especially masking/shadowing. */
import { fft, type Pair, tau } from "./spectral";

export function ggxSlope(sx: number, sy: number, alpha: number) {
  return (alpha * alpha) / (Math.PI * (alpha * alpha + sx * sx + sy * sy) ** 2);
}
/** q K1(q) = integral_0^infinity exp(-t-q^2/(4t)) dt.
 * Log-domain Simpson quadrature avoids importing a special-function library.
 * This offline evaluator is numerical, not a special-function error certificate. */
export function ggxCharacteristic(q: number, steps = 512) {
  if (q === 0) return 1;
  const lo = -24,
    hi = 4,
    dx = (hi - lo) / steps;
  let sum = 0;
  for (let i = 0; i <= steps; i++) {
    const v = lo + i * dx,
      t = Math.exp(v);
    sum += (i === 0 || i === steps ? 1 : i % 2 ? 4 : 2) * Math.exp(v - t - (q * q) / (4 * t));
  }
  return (sum * dx) / 3;
}
export function besselJ0(z: number, steps = 256) {
  let sum = 0;
  for (let i = 0; i < steps; i++) sum += Math.cos(z * Math.cos((tau * (i + 0.5)) / steps));
  return sum / steps;
}
export interface SlopeAtlas {
  size: number;
  period: number;
  alpha: number;
  values: Float64Array;
}
export function compileSlopeAtlas(slopes: Pair[], alpha: number, size = 256, period = 2): SlopeAtlas {
  const re = new Float64Array(size * size),
    im = new Float64Array(size * size);
  const radial = new Map<number, number>();
  for (let y = 0; y < size; y++)
    for (let x = 0; x < size; x++) {
      // A single Nyquist bin has no distinct conjugate on an off-grid torus.
      // Remove both axes so the finite characteristic polynomial is Hermitian.
      if (x === size / 2 || y === size / 2) continue;
      const m = x < size / 2 ? x : x - size,
        n = y < size / 2 ? y : y - size;
      const squared = m * m + n * n;
      let micro = radial.get(squared);
      if (micro === undefined) {
        micro = ggxCharacteristic(((alpha * tau) / period) * Math.sqrt(squared));
        radial.set(squared, micro);
      }
      let phi = micro;
      for (const slope of slopes) phi *= besselJ0((tau / period) * (m * slope[0] + n * slope[1]));
      re[y * size + x] = phi;
    }
  const rowRe = new Float64Array(size),
    rowIm = new Float64Array(size);
  for (let y = 0; y < size; y++) {
    rowRe.set(re.subarray(y * size, (y + 1) * size));
    rowIm.fill(0);
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
    for (let y = 0; y < size; y++) re[y * size + x] = rowRe[y] / (period * period);
  }
  return { size, period, alpha, values: re };
}
export function sampleSlopeAtlas(atlas: SlopeAtlas, sx: number, sy: number) {
  const x = ((((sx / atlas.period) % 1) + 1) % 1) * atlas.size,
    y = ((((sy / atlas.period) % 1) + 1) % 1) * atlas.size;
  const ix = Math.floor(x),
    iy = Math.floor(y),
    fx = x - ix,
    fy = y - iy;
  const at = (a: number, b: number) => atlas.values[(b % atlas.size) * atlas.size + (a % atlas.size)];
  return (
    (1 - fy) * ((1 - fx) * at(ix, iy) + fx * at(ix + 1, iy)) +
    fy * ((1 - fx) * at(ix, iy + 1) + fx * at(ix + 1, iy + 1))
  );
}
export function sampledSlopeDensity(slopes: Pair[], alpha: number, sx: number, sy: number, side = 256) {
  if (slopes.length !== 2) throw new Error("This independent reference integrates two phases");
  let sum = 0;
  for (let y = 0; y < side; y++)
    for (let x = 0; x < side; x++) {
      const ca = Math.cos((tau * (x + 0.5)) / side),
        cb = Math.cos((tau * (y + 0.5)) / side);
      sum += ggxSlope(
        sx - slopes[0][0] * ca - slopes[1][0] * cb,
        sy - slopes[0][1] * ca - slopes[1][1] * cb,
        alpha,
      );
    }
  return sum / (side * side);
}
/** Bound on periodic-image contamination, for |query| + max |macro slope| <= B.
 * Every infinity-norm lattice ring r has 8r copies at distance >= period*r-B. */
export function periodizationBound(alpha: number, period: number, B: number) {
  if (B >= period) return Infinity;
  let sum = 0;
  for (let r = 1; r < 64; r++) sum += (8 * r * alpha * alpha) / (Math.PI * (period * r - B) ** 4);
  sum += (8 * alpha * alpha) / (Math.PI * (period - B / 64) ** 4) / (2 * 63 ** 2);
  return sum;
}
