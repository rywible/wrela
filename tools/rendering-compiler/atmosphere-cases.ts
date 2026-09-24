import type { Camera, Vec3 } from "@wrela/model";

export type AtmosphereCase = { id: string; height: number; sunElevation: number; exposure: number };
/** Degrees above the local geometric horizon. One frame per case, no settling loop. */
export const ATMOSPHERE_CASES: readonly AtmosphereCase[] = [
  { id: "day", height: 2, sunElevation: 45, exposure: 1 },
  { id: "sunset", height: 2, sunElevation: 1, exposure: 1 },
  { id: "horizon-above", height: 2, sunElevation: 0.01, exposure: 1 },
  { id: "horizon-below", height: 2, sunElevation: -0.01, exposure: 1 },
  { id: "high-altitude", height: 60000, sunElevation: 45, exposure: 1 },
  { id: "planet-shadow", height: 2, sunElevation: -90, exposure: 1 },
  { id: "exposure-low", height: 2, sunElevation: 45, exposure: 0.25 },
  { id: "exposure-high", height: 2, sunElevation: 45, exposure: 4 },
];
export function atmosphereCamera(height: number): Camera {
  return { position: [0, height, 0], target: [0, height + 0.2, -1], fov: 50 };
}
export function atmosphereSun(degrees: number): Vec3 {
  const angle = (degrees * Math.PI) / 180;
  return [0, Math.sin(angle), -Math.cos(angle)];
}
export type AtmosphereSample = {
  id: string;
  meanRgb: number[];
  meanLuminance: number;
  maximum: number;
  displayMean: number;
  linearPixels: number;
};
export function summarizeAtmosphere(id: string, pixels: Float32Array, display: Uint8Array): AtmosphereSample {
  if (!pixels.length || pixels.length % 4 || display.length !== pixels.length)
    throw Error(`${id}: invalid readback dimensions`);
  const meanRgb = [0, 0, 0];
  let maximum = 0;
  let displaySum = 0;
  for (let i = 0; i < pixels.length; i += 4) {
    for (let channel = 0; channel < 4; channel++) {
      const value = pixels[i + channel];
      if (!Number.isFinite(value) || value < 0)
        throw Error(`${id}: nonfinite/negative HDR at ${i + channel}`);
      if (channel < 3) {
        meanRgb[channel] += value / (pixels.length / 4);
        maximum = Math.max(maximum, value);
        displaySum += display[i + channel];
      }
    }
  }
  return {
    id,
    meanRgb,
    maximum,
    meanLuminance: meanRgb[0] * 0.2126 + meanRgb[1] * 0.7152 + meanRgb[2] * 0.0722,
    displayMean: displaySum / ((pixels.length / 4) * 3 * 255),
    linearPixels: pixels.length / 4,
  };
}
/** Behavioral acceptance, not a radiometric accuracy certificate. */
export function validateAtmosphereSamples(samples: AtmosphereSample[], exposureRelativeRms: number) {
  const get = (id: string) => {
    const sample = samples.find((value) => value.id === id);
    if (!sample) throw Error(`Missing atmosphere case ${id}`);
    return sample;
  };
  const day = get("day"),
    sunset = get("sunset"),
    high = get("high-altitude"),
    night = get("planet-shadow");
  if (!(day.meanLuminance > 1e-6)) throw Error("Day atmosphere is black");
  if (!(night.maximum < day.maximum * 1e-4 + 1e-7)) throw Error("Planet shadow leaks sunlight");
  const relative = (a: number, b: number) => Math.abs(a - b) / Math.max(a, b, 1e-8);
  if (relative(high.meanLuminance, day.meanLuminance) < 0.05)
    throw Error("Altitude did not affect atmosphere");
  if (relative(sunset.meanLuminance, day.meanLuminance) < 0.05)
    throw Error("Sun elevation did not affect atmosphere");
  const horizonAbove = get("horizon-above"),
    horizonBelow = get("horizon-below");
  if (relative(horizonAbove.meanLuminance, horizonBelow.meanLuminance) > 0.15)
    throw Error("Atmosphere is discontinuous across the horizon");
  if (!(Number.isFinite(exposureRelativeRms) && exposureRelativeRms <= 1e-6))
    throw Error("Display exposure changed scene-linear atmosphere radiance");
  if (!(get("exposure-high").displayMean > get("exposure-low").displayMean + 0.01))
    throw Error("Display exposure did not brighten the atmosphere");
  return {
    finiteNonnegative: true,
    planetShadow: true,
    altitudeResponse: true,
    sunsetResponse: true,
    horizonContinuity: true,
    exposureLinearInvariant: true,
    exposureDisplayResponse: true,
  };
}
