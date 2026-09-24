import type { Vec3 } from "@wrela/model";
import { type IndirectGeometry, indirectDot, traceIndirectRay } from "./indirect-query";
import {
  type RadianceEmitters,
  radianceEmitterPdf,
  radianceMisWeight,
  sampleRadianceEmitter,
} from "./radiance-emission";

const directions = new Map<number, Vec3[]>();
function hemisphereDirections(samples: number): Vec3[] {
  let result = directions.get(samples);
  if (!result) {
    result = Array.from({ length: samples }, (_, i) => {
      const u = (i + 0.5) / samples,
        phi = 2 * Math.PI * (((i + 0.5) * 0.61803398875) % 1);
      return [Math.sqrt(u) * Math.cos(phi), Math.sqrt(u) * Math.sin(phi), Math.sqrt(1 - u)] as Vec3;
    });
    directions.set(samples, result);
  }
  return result;
}

/** A triangle's shadow volume is convex. If all eight corners of the sources'
 * box lie beyond that triangle inside its edge planes, every source point is
 * occluded. A center ray alone never supplies this certificate. */
export function emissionBoxOccluded(
  geometry: IndirectGeometry,
  emitters: RadianceEmitters,
  position: Vec3,
): { blocked: boolean; rays: number } {
  const center = emitters.min.map((v, a) => (v + emitters.max[a]) * 0.5 - position[a]) as Vec3;
  const distance = Math.hypot(...center);
  if (distance < 0.003) return { blocked: false, rays: 0 };
  const hit = traceIndirectRay(geometry, position, center.map((v) => v / distance) as Vec3, distance - 0.003);
  if (!hit) return { blocked: false, rays: 1 };
  const t = geometry.triangles[hit.triangle];
  const vertices = [t.a, t.a.map((v, a) => v + t.ab[a]), t.a.map((v, a) => v + t.ac[a])].map(
    (p) => p.map((v, a) => v - position[a]) as Vec3,
  );
  const side = indirectDot(t.normal, vertices[0]);
  if (Math.abs(side) < 1e-9) return { blocked: false, rays: 1 };
  const planes = vertices.map((a, i) => {
    const b = vertices[(i + 1) % 3];
    const n: Vec3 = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    const sign = Math.sign(indirectDot(n, vertices[(i + 2) % 3]));
    return n.map((v) => v * sign) as Vec3;
  });
  for (let corner = 0; corner < 8; corner++) {
    const point = position.map((v, a) => (corner & (1 << a) ? emitters.max[a] : emitters.min[a]) - v) as Vec3;
    if (
      indirectDot(t.normal, point) / side <= 1.00001 ||
      planes.some((n) => indirectDot(n, point) < 1e-8 * Math.hypot(...n))
    )
      return { blocked: false, rays: 1 };
  }
  return { blocked: true, rays: 1 };
}

/** Direct emitted irradiance / pi at the actual receiver. Source-area and
 * cosine sampling complement each other for small distant and large nearby
 * emitters. This term has no spatial probe interpolation or SH convolution.
 * The caller supplies a point just outside the receiving surface. */
export function* localEmissionSteps(
  geometry: IndirectGeometry,
  emitters: RadianceEmitters,
  position: Vec3,
  normal: Vec3,
  samples = 32,
): Generator<void, { value: Vec3; rays: number }> {
  const value: Vec3 = [0, 0, 0];
  let rays = 0;
  if (!emitters.entries.length) return { value, rays };
  // A source wholly below the tangent plane contributes exactly zero. This
  // geometric proof also avoids thousands of useless back-side receiver rays.
  const farthest = position.map((v, a) => (normal[a] >= 0 ? emitters.max[a] : emitters.min[a]) - v) as Vec3;
  if (indirectDot(farthest, normal) <= 1e-7) return { value, rays };
  const occlusion = emissionBoxOccluded(geometry, emitters, position);
  rays += occlusion.rays;
  if (occlusion.blocked) return { value, rays };
  const tangent: Vec3 = Math.abs(normal[1]) < 0.9 ? [normal[2], 0, -normal[0]] : [0, -normal[2], normal[1]];
  const length = Math.hypot(...tangent);
  for (let a = 0; a < 3; a++) tangent[a] /= length;
  const bitangent: Vec3 = [
    normal[1] * tangent[2] - normal[2] * tangent[1],
    normal[2] * tangent[0] - normal[0] * tangent[2],
    normal[0] * tangent[1] - normal[1] * tangent[0],
  ];
  const localDirections = hemisphereDirections(samples);
  for (let i = 0; i < samples; i++) {
    const source = sampleRadianceEmitter(geometry, emitters, position, i, samples, 0);
    if (source) {
      const pdf = Math.max(0, indirectDot(normal, source.direction)) / Math.PI;
      if (pdf > 0) {
        rays++;
        if (!traceIndirectRay(geometry, position, source.direction, source.distance - 0.003)) {
          const weight = (pdf * radianceMisWeight(source.pdf, pdf)) / (source.pdf * samples);
          for (let c = 0; c < 3; c++) value[c] += source.emission[c] * weight;
        }
      }
    }
    const d = localDirections[i];
    const direction = normal.map((n, a) => tangent[a] * d[0] + bitangent[a] * d[1] + n * d[2]) as Vec3;
    const hit = traceIndirectRay(geometry, position, direction);
    rays++;
    if (hit?.emission) {
      const pdf = Math.max(0, indirectDot(normal, direction)) / Math.PI;
      const weight =
        radianceMisWeight(
          pdf,
          radianceEmitterPdf(geometry, emitters, hit.triangle, hit.distance, direction),
        ) / samples;
      for (let c = 0; c < 3; c++) value[c] += hit.emission[c] * weight;
    }
    if (i % 8 === 7) yield;
  }
  return { value, rays };
}

export function localEmission(...args: Parameters<typeof localEmissionSteps>) {
  const steps = localEmissionSteps(...args);
  let item = steps.next();
  while (!item.done) item = steps.next();
  return item.value;
}
