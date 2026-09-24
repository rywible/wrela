import { z } from "zod";
import { cross, dot, hash32, normalize, scale, sub, type Vec3 } from "./math";

const unit = z.number().finite().min(0).max(1);
const point = z.tuple([z.number().finite(), z.number().finite(), z.number().finite()]);
const direction = point.refine(
  (value) => Number.isFinite(Math.hypot(...value)) && Math.hypot(...value) > 1e-6,
  "History frame direction must be finite and nonzero",
);
/** Shared environmental meaning. Distances use metres along the local up axis from origin. */
export const surfaceHistorySchema = z.object({
  seed: z.number().int().min(0).max(0x7fffffff).default(73),
  origin: point.default([0, 0, 0]),
  up: direction.default([0, 1, 0]),
  rainDirection: direction.default([0.35, -1, 0.55]),
  groundHeight: z.number().finite().default(0),
  waterHeight: z.number().finite().optional(),
  contactRise: z.number().finite().min(0.01).max(10).default(0.55),
  runoffSpacing: z.number().finite().min(0.02).max(20).default(0.42),
  ageYears: z.number().finite().min(0).max(2000).default(140),
  rainExposure: unit.default(0.65),
  prevailingWetness: unit.default(0.18),
  damage: unit.default(0.28),
});
export type SurfaceHistory = z.infer<typeof surfaceHistorySchema>;
export type SurfaceHistorySample = {
  position: Vec3;
  normal: Vec3;
  /** Architectural overhangs and neighboring plants can supply an explicit shelter estimate. */
  shelter?: number;
  /** Override automatic ground/water contact when sampling a grounded world member. */
  contact?: number;
};
export type SurfaceHistorySignals = {
  contact: number;
  exposure: number;
  runoff: number;
  age: number;
  wetness: number;
  dirt: number;
  damage: number;
  moss: number;
  debris: number;
};
const unitValue = (value: number) => Math.max(0, Math.min(1, value));
function coherentNoise(value: number, seed: number): number {
  const integer = Math.floor(value),
    fraction = value - integer;
  const a = (hash32(integer ^ seed) >>> 0) / 0x1_0000_0000;
  const b = (hash32((integer + 1) ^ seed) >>> 0) / 0x1_0000_0000;
  return a + (b - a) * fraction * fraction * (3 - 2 * fraction);
}
/** One deterministic cause model drives surfaces, erosion, loose stone and plant communities. */
export function sampleSurfaceHistory(
  source: SurfaceHistory,
  sample: SurfaceHistorySample,
): SurfaceHistorySignals {
  const history = surfaceHistorySchema.parse(source);
  if (
    ![...sample.position, ...sample.normal, sample.shelter ?? 0, sample.contact ?? 0].every(Number.isFinite)
  )
    throw new Error("Surface history samples must be finite");
  const up = normalize(history.up),
    rain = normalize(history.rainDirection),
    normal = normalize(sample.normal);
  if (Math.hypot(...normal) < 0.5) throw new Error("Surface history sample normal must be nonzero");
  const local = sub(sample.position, history.origin),
    height = dot(local, up);
  const shelter = unitValue(sample.shelter ?? 0);
  const ground = Math.exp(-Math.max(0, height - history.groundHeight) / history.contactRise);
  const water =
    history.waterHeight === undefined
      ? 0
      : Math.exp(-Math.max(0, height - history.waterHeight) / (history.contactRise * 0.65));
  const contact = unitValue(sample.contact ?? Math.max(ground, water));
  const exposure =
    history.rainExposure * (1 - shelter) * (0.2 + 0.8 * Math.max(0, dot(normal, scale(rain, -1))));
  const crossRain = cross(up, rain);
  const across = normalize(
    Math.hypot(...crossRain) > 1e-6 ? crossRain : cross(up, Math.abs(up[0]) < 0.9 ? [1, 0, 0] : [0, 0, 1]),
  );
  const streak = coherentNoise(dot(local, across) / history.runoffSpacing + height * 0.035, history.seed);
  const runoff = exposure * (1 - Math.abs(dot(normal, up)) * 0.8) * (0.2 + 0.8 * streak * streak);
  const age = 1 - Math.exp(-history.ageYears / 95);
  const wetness = unitValue(
    history.prevailingWetness * (0.3 + shelter * 0.7) + contact * 0.48 + runoff * 0.58,
  );
  const dirt = unitValue(age * (contact * 0.5 + shelter * 0.24 + runoff * 0.18));
  const damage = unitValue(history.damage * (0.38 + age * 0.32 + exposure * 0.3) + age * runoff * 0.16);
  const moss = unitValue(age * wetness * (0.45 + shelter * 0.35 + contact * 0.2) * (1 - exposure * 0.65));
  const debris = unitValue(damage * (0.25 + contact * 0.75) * (0.55 + age * 0.45));
  return { contact, exposure, runoff, age, wetness, dirt, damage, moss, debris };
}
export function alpineSurfaceHistory(overrides: Partial<SurfaceHistory> = {}): SurfaceHistory {
  return surfaceHistorySchema.parse({ ...overrides });
}
