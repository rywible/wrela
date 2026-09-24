import type { Vec3 } from "./math";

/** The optional per-vertex botanical motion attribute used by the GPU renderer. */
export type VegetationMotionWeights = readonly [
  branchPhase: number,
  branchAmplitude: number,
  leafPhase: number,
  leafAmplitude: number,
];

/** CPU reference for scene.wgsl. Both oscillations use unit directions, so the sum
 * of their amplitudes is a conservative displacement bound at maximum wind. */
export function vegetationMotionOffset(
  weights: VegetationMotionWeights,
  world: Vec3,
  time: number,
  wind: Vec3,
  response: number,
  phase = 0,
  instanceScale = 1,
): Vec3 {
  const speed = Math.hypot(wind[0], wind[2]);
  if (speed === 0 || response <= 0) return [0, 0, 0];
  const x = wind[0] / speed,
    z = wind[2] / speed;
  const strength = (Math.min(speed, 10) / 10) * Math.max(0, Math.min(2, response)) * instanceScale;
  const branch = Math.sin(time * 2.1 + phase + world[0] * 0.17 + world[2] * 0.23 + weights[0]) * weights[1];
  const leaf = (Math.sin(time * 7.3 + weights[2]) * weights[3]) / Math.sqrt(1 + 0.35 ** 2);
  return [(x * branch - z * leaf) * strength, leaf * 0.35 * strength, (z * branch + x * leaf) * strength];
}

export function vegetationMotionEnvelope(weights: ArrayLike<number> | undefined): number {
  let amplitude = 0;
  for (let i = 0; weights && i < weights.length; i += 4)
    amplitude = Math.max(amplitude, weights[i + 1] + weights[i + 3]);
  return amplitude * 2;
}
