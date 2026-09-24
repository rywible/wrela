import type { Camera, RenderCompleteness, Vec3 } from "@wrela/model";

import { WINTER_TRAJECTORY } from "./winter-scene";

export const ACCEPTANCE_VERSION = "rendering-compiler-acceptance-2";
export const TRAJECTORY = WINTER_TRAJECTORY;
export const PRIMITIVE_CAMERAS: { name: string; camera: Camera }[] = [
  { name: "close-up", camera: { position: [3, 1.5, 4], target: [0, 0.8, 0], fov: 42 } },
  { name: "grazing", camera: { position: [4, 0.82, 0.04], target: [0, 0.8, 0], fov: 42 } },
  { name: "inside", camera: { position: [0, 0.8, 0], target: [2, 0.9, 0], fov: 60 } },
  { name: "near-plane", camera: { position: [0, 0.8, 0.91], target: [0, 0.8, -1], fov: 60 } },
  { name: "distant", camera: { position: [480, 180, 540], target: [0, 0.8, 0], fov: 42 } },
];
export type AcceptanceScenario = "winter-valley" | "primitive" | "water" | "lighting" | "visibility";
export type AcceptanceVariant =
  | "production"
  | "parametric-control"
  | "visibility-control"
  | "water-reference";
export const VARIANTS: Record<
  AcceptanceVariant,
  { geometry?: "parametric"; water?: "reference"; visibility?: boolean }
> = {
  production: {},
  "parametric-control": { geometry: "parametric" },
  "visibility-control": { visibility: false },
  "water-reference": { water: "reference" },
};
export function trajectoryFrame(frame: number, frameCount: number) {
  if (
    !Number.isInteger(frameCount) ||
    frameCount < 2 ||
    !Number.isInteger(frame) ||
    frame < 0 ||
    frame >= frameCount
  )
    throw new RangeError("Invalid trajectory frame");
  const phase = (frame / (frameCount - 1)) * (TRAJECTORY.length - 1);
  const index = Math.min(TRAJECTORY.length - 2, Math.floor(phase));
  const a = TRAJECTORY[index],
    b = TRAJECTORY[index + 1],
    weight = phase - index;
  const lerp = (x: readonly number[], y: readonly number[]): Vec3 =>
    x.map((v, axis) => v + (y[axis] - v) * weight) as Vec3;
  return {
    time: a.time + (b.time - a.time) * weight,
    camera: { position: lerp(a.position, b.position), target: lerp(a.target, b.target), fov: 50 },
    sunDirection: lerp(a.sun, b.sun),
  };
}
/** Accounting is a multiset equality, not merely the renderer's complete boolean. */
export function assertCompleteIdentities(expected: readonly string[], actual: RenderCompleteness) {
  if (!actual.complete || actual.uploading.length || actual.rejected.length)
    throw new Error(`Incomplete capture: ${JSON.stringify(actual)}`);
  const identities = [...actual.rendered, ...actual.culled];
  if (new Set(expected).size !== expected.length || new Set(identities).size !== identities.length)
    throw new Error("Duplicate identities in acceptance accounting");
  const reported = new Set(identities);
  if (identities.length !== expected.length || expected.some((id) => !reported.has(id)))
    throw new Error(
      `Missing or extra identities: expected ${expected.length}, accounted ${identities.length}`,
    );
}
export function compareLinear(
  reference: ArrayLike<number>,
  candidate: ArrayLike<number>,
  denominatorFloor = 0.01,
) {
  if (
    reference.length !== candidate.length ||
    reference.length === 0 ||
    reference.length % 4 ||
    !Number.isFinite(denominatorFloor) ||
    denominatorFloor <= 0
  )
    throw new Error("Invalid linear image comparison");
  let squared = 0,
    energy = 0,
    maximum = 0,
    highlightMaximum = 0,
    peak = 0;
  const differences: number[] = [];
  for (let i = 0; i < reference.length; i++) {
    if (!Number.isFinite(reference[i]) || !Number.isFinite(candidate[i]))
      throw new Error("NaN/Inf in rendering output");
    if (i % 4 !== 3) peak = Math.max(peak, reference[i]);
  }
  for (let i = 0; i < reference.length; i++) {
    if (i % 4 === 3) continue;
    const delta = Math.abs(candidate[i] - reference[i]);
    squared += delta * delta;
    energy += reference[i] * reference[i];
    maximum = Math.max(maximum, delta);
    if (reference[i] >= peak * 0.9) {
      highlightMaximum = Math.max(highlightMaximum, delta);
      differences.push(delta);
    }
  }
  differences.sort((a, b) => a - b);
  const channels = reference.length * 0.75;
  const rms = Math.sqrt(squared / channels),
    referenceRms = Math.sqrt(energy / channels);
  return {
    rms,
    relativeRms: rms / Math.max(referenceRms, denominatorFloor),
    referenceRms,
    denominatorFloor,
    maximum,
    highlightMaximum,
    highlightP95: differences[Math.max(0, Math.ceil(differences.length * 0.95) - 1)],
    highlightThreshold: peak * 0.9,
  };
}
