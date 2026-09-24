import type { Vec3 } from "@wrela/model";
import { type IndirectGeometry, indirectDot } from "./indirect-query";
import { radianceRandom } from "./radiance-sequence";

type Emitter = { triangle: number; cumulative: number; areaPdf: number };
export type RadianceEmitters = {
  entries: Emitter[];
  areaPdfs: Map<number, number>;
  power: number;
  min: Vec3;
  max: Vec3;
};
const distributions = new WeakMap<IndirectGeometry, RadianceEmitters>();

/** Emissive source geometry supplies an importance distribution. No hand-placed
 * surrogate light or authoring switch is required. */
export function* radianceEmitterSteps(geometry: IndirectGeometry): Generator<void, RadianceEmitters> {
  const existing = distributions.get(geometry);
  if (existing) return existing;
  // A moving opaque door changes visibility, not the source distribution.
  // Keep that immutable compiler product instead of rescanning the world.
  if (geometry.staticGeometry) {
    let unchanged = true;
    for (let i = geometry.staticGeometry.triangles.length; i < geometry.triangles.length; i++) {
      if (geometry.triangles[i].emission?.some((v) => v > 0)) {
        unchanged = false;
        break;
      }
      if (i % 512 === 511) yield;
    }
    if (unchanged) {
      const inherited = yield* radianceEmitterSteps(geometry.staticGeometry);
      distributions.set(geometry, inherited);
      return inherited;
    }
  }
  const entries: Emitter[] = [],
    areaPdfs = new Map<number, number>();
  let power = 0;
  const min: Vec3 = [Infinity, Infinity, Infinity],
    max: Vec3 = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < geometry.triangles.length; i++) {
    const t = geometry.triangles[i],
      energy = Math.max(...(t.emission ?? [0, 0, 0]));
    if (energy > 0) {
      const b = t.ab,
        c = t.ac;
      const area =
        Math.hypot(b[1] * c[2] - b[2] * c[1], b[2] * c[0] - b[0] * c[2], b[0] * c[1] - b[1] * c[0]) * 0.5;
      if (area > 1e-15) {
        power += area * energy;
        entries.push({ triangle: i, cumulative: power, areaPdf: energy });
        for (let a = 0; a < 3; a++) {
          min[a] = Math.min(min[a], t.min[a]);
          max[a] = Math.max(max[a], t.max[a]);
        }
      }
    }
    if (i % 512 === 511) yield;
  }
  for (const entry of entries) {
    entry.areaPdf /= power;
    areaPdfs.set(entry.triangle, entry.areaPdf);
  }
  const result = { entries, areaPdfs, power, min, max };
  distributions.set(geometry, result);
  return result;
}

export function radianceEmitterPdf(
  geometry: IndirectGeometry,
  emitters: RadianceEmitters,
  triangle: number,
  distance: number,
  direction: Vec3,
) {
  const areaPdf = emitters.areaPdfs.get(triangle) ?? 0;
  if (!areaPdf) return 0;
  const cosine = Math.abs(indirectDot(geometry.triangles[triangle].normal, direction));
  return cosine > 1e-12 ? (areaPdf * distance * distance) / cosine : 0;
}
export const radianceMisWeight = (pdf: number, other: number) => {
  const ratio = other / Math.max(pdf, 1e-30);
  return 1 / (1 + ratio * ratio);
};

export function sampleRadianceEmitter(
  geometry: IndirectGeometry,
  emitters: RadianceEmitters,
  origin: Vec3,
  index: number,
  count: number,
  bounce: number,
) {
  if (!emitters.entries.length) return;
  // At reflected vertices, source choice must not share the primary sphere's
  // monotonically stratified coordinate. That correlation biases transport.
  const selection = (bounce ? radianceRandom(index, bounce * 8) : (index + 0.5) / count) * emitters.power;
  let lo = 0,
    hi = emitters.entries.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (selection < emitters.entries[mid].cumulative) hi = mid;
    else lo = mid + 1;
  }
  const entry = emitters.entries[lo],
    triangle = geometry.triangles[entry.triangle];
  const u = Math.sqrt(bounce ? radianceRandom(index, bounce * 8 + 1) : ((index + 0.5) * 0.754877666) % 1);
  const v = bounce ? radianceRandom(index, bounce * 8 + 2) : ((index + 0.5) * 0.569840296) % 1;
  const delta = triangle.a.map(
    (value, a) => value + triangle.ab[a] * (u * (1 - v)) + triangle.ac[a] * (u * v) - origin[a],
  ) as Vec3;
  const distance = Math.hypot(...delta);
  if (distance < 0.002) return;
  const direction = delta.map((value) => value / distance) as Vec3;
  const pdf = radianceEmitterPdf(geometry, emitters, entry.triangle, distance, direction);
  if (!(pdf > 0)) return;
  return { direction, distance, pdf, emission: triangle.emission ?? ([0, 0, 0] as Vec3) };
}
