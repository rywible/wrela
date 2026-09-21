/** Exact first and second slope moments of Wrela's finite analytic wave sum,
 * integrated over an affine pixel footprint and a uniform shutter interval.
 * The moments are exact in real arithmetic. A BRDF fitted to these moments is
 * still an approximation; this module does not claim exact filtered radiance. */
import type { WaterDefinition } from "@wrela/model";

type V2 = [number, number];
type Wave = { k: V2; omega: number; phase: number; slope: V2 };
export type Footprint = { dx: V2; dy: V2; shutter: number };
export type Moments = { mean: V2; covariance: [number, number, number] };
export function compileWaves(waves: WaterDefinition["waves"]): Wave[] {
  return waves.map((w) => {
    const frequency = (2 * Math.PI) / w.wavelength;
    const k: V2 = [frequency * Math.cos(w.direction), frequency * Math.sin(w.direction)];
    return {
      k,
      omega: -frequency * w.speed,
      phase: w.phase,
      slope: [k[0] * w.amplitude, k[1] * w.amplitude],
    };
  });
}
const sinc = (x: number) => (Math.abs(x) < 1e-4 ? 1 - (x * x) / 6 + x ** 4 / 120 : Math.sin(x) / x);
function footprintIntegral(kx: number, kz: number, omega: number, footprint: Footprint): number {
  return (
    sinc((kx * footprint.dx[0] + kz * footprint.dx[1]) / 2) *
    sinc((kx * footprint.dy[0] + kz * footprint.dy[1]) / 2) *
    sinc((omega * footprint.shutter) / 2)
  );
}
export function waveMoments(waves: Wave[], x: number, z: number, t: number, footprint: Footprint): Moments {
  const phases = waves.map((w) => w.k[0] * x + w.k[1] * z + w.omega * t + w.phase);
  const mean: V2 = [0, 0],
    second = [0, 0, 0];
  for (let i = 0; i < waves.length; i++) {
    const a = waves[i],
      average = Math.cos(phases[i]) * footprintIntegral(a.k[0], a.k[1], a.omega, footprint);
    mean[0] += a.slope[0] * average;
    mean[1] += a.slope[1] * average;
    for (let j = i; j < waves.length; j++) {
      const b = waves[j];
      const cosineProduct =
        0.5 *
        (Math.cos(phases[i] - phases[j]) *
          footprintIntegral(a.k[0] - b.k[0], a.k[1] - b.k[1], a.omega - b.omega, footprint) +
          Math.cos(phases[i] + phases[j]) *
            footprintIntegral(a.k[0] + b.k[0], a.k[1] + b.k[1], a.omega + b.omega, footprint));
      const weight = i === j ? 1 : 2;
      second[0] += weight * a.slope[0] * b.slope[0] * cosineProduct;
      second[1] +=
        (i === j ? a.slope[0] * a.slope[1] : a.slope[0] * b.slope[1] + b.slope[0] * a.slope[1]) *
        cosineProduct;
      second[2] += weight * a.slope[1] * b.slope[1] * cosineProduct;
    }
  }
  return {
    mean,
    covariance: [second[0] - mean[0] ** 2, second[1] - mean[0] * mean[1], second[2] - mean[1] ** 2],
  };
}
export function sampledMoments(
  waves: Wave[],
  x: number,
  z: number,
  t: number,
  footprint: Footprint,
  spatialSamples: number,
  temporalSamples = 1,
): Moments {
  const mean: V2 = [0, 0],
    second = [0, 0, 0];
  for (let iy = 0; iy < spatialSamples; iy++)
    for (let ix = 0; ix < spatialSamples; ix++)
      for (let it = 0; it < temporalSamples; it++) {
        const u = (ix + 0.5) / spatialSamples - 0.5,
          v = (iy + 0.5) / spatialSamples - 0.5,
          time = t + ((it + 0.5) / temporalSamples - 0.5) * footprint.shutter;
        const px = x + u * footprint.dx[0] + v * footprint.dy[0],
          pz = z + u * footprint.dx[1] + v * footprint.dy[1];
        let sx = 0,
          sz = 0;
        for (const w of waves) {
          const c = Math.cos(w.k[0] * px + w.k[1] * pz + w.omega * time + w.phase);
          sx += w.slope[0] * c;
          sz += w.slope[1] * c;
        }
        mean[0] += sx;
        mean[1] += sz;
        second[0] += sx * sx;
        second[1] += sx * sz;
        second[2] += sz * sz;
      }
  const count = spatialSamples ** 2 * temporalSamples;
  mean[0] /= count;
  mean[1] /= count;
  return {
    mean,
    covariance: [
      second[0] / count - mean[0] ** 2,
      second[1] / count - mean[0] * mean[1],
      second[2] / count - mean[1] ** 2,
    ],
  };
}
