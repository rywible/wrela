import type { RadianceLightingField, Vec3 } from "@wrela/model";
import { indirectSH } from "./indirect-probes";
import {
  type IndirectGeometry,
  indirectDot,
  indirectHemisphere,
  indirectOffset,
  traceIndirectRay,
} from "./indirect-query";
import {
  radianceEmitterPdf,
  radianceEmitterSteps,
  radianceMisWeight,
  sampleRadianceEmitter,
} from "./radiance-emission";
import { accumulateRadiancePath, type RadiancePath } from "./radiance-path";
import { radianceRandom } from "./radiance-sequence";

const distance = (a: Vec3, b: Vec3) => Math.hypot(...a.map((v, i) => v - b[i]));

/** Shared path estimator for cached probes and independent surface-point
 * references. The optional observation is before angular/spatial interpolation,
 * so references can integrate the actual cosine rather than the SH approximation. */
export function* traceRadianceSampleSteps(
  geometry: IndirectGeometry,
  position: Vec3,
  sample: number,
  rayCount: number,
  lights: RadianceLightingField["lights"],
  transfer: Float32Array,
  skyVisibility: Float32Array,
  observe?: (direction: Vec3, incident: Float64Array) => void,
  directEmission?: Float32Array,
  observeSegment?: (origin: Vec3, direction: Vec3, length: number, path: number) => void,
  paths?: {
    first?: number;
    end?: number;
    project?: boolean;
    /** Surface-only quadrature uses cosine-weighted primary directions. Its
     * observer integrates irradiance directly; it must not publish a spherical
     * probe using the sphere's projection measure. */
    normal?: Vec3;
    observe?: (index: number, path: RadiancePath) => void;
  },
): Generator<void, number> {
  if (
    paths &&
    (!Number.isInteger(paths.first ?? 0) ||
      !Number.isInteger(paths.end ?? rayCount) ||
      (paths.first ?? 0) < 0 ||
      (paths.end ?? rayCount) > rayCount ||
      (paths.first ?? 0) > (paths.end ?? rayCount))
  )
    throw Error("Invalid radiance path range");
  if (
    paths?.normal &&
    (paths.project !== false ||
      observe ||
      paths.normal.some((v) => !Number.isFinite(v)) ||
      Math.abs(Math.hypot(...paths.normal) - 1) > 1e-6)
  )
    throw Error("Surface radiance paths require a unit normal and disabled sphere projection");
  const emitters = yield* radianceEmitterSteps(geometry);
  let pathIndex = 0;
  const trace: typeof traceIndirectRay = observeSegment
    ? (...args) => {
        const hit = traceIndirectRay(...args);
        observeSegment(args[1], args[2], hit?.distance ?? args[3] ?? Infinity, pathIndex);
        return hit;
      }
    : traceIndirectRay;
  let rays = 0;
  for (let r = paths?.first ?? 0; r < (paths?.end ?? rayCount); r++) {
    pathIndex = r;
    const y = 1 - (2 * (r + 0.5)) / rayCount,
      radius = Math.sqrt(1 - y * y),
      angle = 2 * Math.PI * ((r * 0.61803398875) % 1);
    const direction: Vec3 = paths?.normal
      ? indirectHemisphere(paths.normal, (r + 0.5) / rayCount, (r * 0.61803398875) % 1)
      : [radius * Math.cos(angle), y, radius * Math.sin(angle)];
    const primaryPdf = (ray: Vec3) =>
      paths?.normal ? Math.max(0, indirectDot(paths.normal, ray)) / Math.PI : 1 / (4 * Math.PI);
    const incident = new Float64Array(27 * 4);
    const path: RadiancePath = { direction, incident, sky: false };
    // Combine source-area sampling with the ordinary direction sample. The
    // power heuristic prevents both double counting and near-emitter spikes.
    const emitter = sampleRadianceEmitter(geometry, emitters, position, r, rayCount, 0);
    if (emitter && primaryPdf(emitter.direction) > 0) {
      rays++;
      if (!trace(geometry, position, emitter.direction, emitter.distance - 0.003)) {
        const weight =
          radianceMisWeight(emitter.pdf, primaryPdf(emitter.direction)) / (emitter.pdf * rayCount);
        path.emitter = {
          direction: emitter.direction,
          energy: emitter.emission.map((v) => v * weight) as Vec3,
        };
        if (observe) {
          const observation = new Float64Array(27 * 4);
          for (let c = 0; c < 3; c++)
            observation[26 * 4 + c] = (emitter.emission[c] * weight * rayCount) / (4 * Math.PI);
          observe(emitter.direction, observation);
        }
      }
    }
    const recordSky = (ray: Vec3, weight: Vec3, bounced: boolean) => {
      indirectSH(ray).forEach((v, k) => {
        for (let c = 0; c < 3; c++) incident[((bounced ? 9 : 0) + k) * 4 + c] += v * weight[c];
      });
    };
    let hit = trace(geometry, position, direction);
    rays++;
    if (!hit) {
      path.sky = true;
      recordSky(direction, [1, 1, 1], false);
    }
    let throughput: Vec3 = [1, 1, 1],
      previousPdf = primaryPdf(direction),
      previousDirection = direction;
    // Compiled transport can follow deep paths without adding runtime shader
    // work. Roulette bounds average preparation cost; the 128-scatter guard
    // also bounds pathological high-reflectance enclosures.
    for (let bounce = 0; hit && bounce <= 128; bounce++) {
      const current = hit;
      if (hit.emission) {
        const emissionWeight = radianceMisWeight(
          previousPdf,
          radianceEmitterPdf(geometry, emitters, hit.triangle, hit.distance, previousDirection),
        );
        for (let c = 0; c < 3; c++) {
          const contribution = throughput[c] * hit.emission[c] * emissionWeight;
          incident[26 * 4 + c] += contribution;
          if (bounce === 0) {
            path.directHit ??= [0, 0, 0];
            path.directHit[c] = contribution;
          }
        }
      }
      // Emission at the terminal hit is still counted. Otherwise a stated
      // N-bounce transport silently retains only N-1 reflected emission paths.
      if (bounce === 128) break;
      const start = indirectOffset(hit.position, hit.normal);
      const reflectedEmitter = sampleRadianceEmitter(geometry, emitters, start, r, rayCount, bounce + 1);
      if (reflectedEmitter) {
        const cosine = Math.max(0, indirectDot(hit.normal, reflectedEmitter.direction));
        if (cosine > 0) {
          rays++;
          if (
            !trace(
              geometry,
              start,
              reflectedEmitter.direction,
              reflectedEmitter.distance - 0.003,
              hit.triangle,
            )
          ) {
            const weight =
              ((cosine / Math.PI) * radianceMisWeight(reflectedEmitter.pdf, cosine / Math.PI)) /
              reflectedEmitter.pdf;
            for (let c = 0; c < 3; c++)
              incident[26 * 4 + c] += throughput[c] * hit.albedo[c] * reflectedEmitter.emission[c] * weight;
          }
        }
      }
      for (let l = 0; l < lights.length; l++) {
        const light = lights[l],
          d = distance(light.position, hit.position);
        if (d < 0.002 || (light.range && d >= light.range)) continue;
        const ray = light.position.map((v, a) => (v - current.position[a]) / d) as Vec3;
        const cosine = Math.max(0, indirectDot(hit.normal, ray));
        if (!cosine) continue;
        rays++;
        if (trace(geometry, start, ray, d - 0.003, hit.triangle)) continue;
        const window = light.range ? Math.max(0, 1 - (d / light.range) ** 4) ** 2 : 1;
        for (let c = 0; c < 3; c++)
          incident[(18 + l) * 4 + c] +=
            (((throughput[c] * hit.albedo[c] * cosine) / Math.PI) * window) / (1 + d * d);
      }
      throughput = throughput.map((v, c) => v * current.albedo[c]) as Vec3;
      const energy = Math.max(...throughput);
      if (energy === 0) break;
      // Source samples above are evaluated before roulette. Surviving BSDF
      // paths carry the compensating weight, preserving expected energy.
      if (bounce >= 5) {
        const survival = Math.min(0.95, energy);
        if (radianceRandom(r, (bounce + 1) * 8 + 5) >= survival) break;
        throughput = throughput.map((v) => v / survival) as Vec3;
      }
      const continuation = indirectHemisphere(
        hit.normal,
        radianceRandom(r, (bounce + 1) * 8 + 3),
        radianceRandom(r, (bounce + 1) * 8 + 4),
      );
      previousPdf = Math.max(0, indirectDot(hit.normal, continuation)) / Math.PI;
      previousDirection = continuation;
      hit = trace(geometry, start, continuation, Infinity, hit.triangle);
      rays++;
      if (!hit) recordSky(continuation, throughput, true);
      if (bounce % 4 === 3) yield;
    }
    observe?.(direction, incident);
    paths?.observe?.(r, path);
    if (paths?.project !== false)
      accumulateRadiancePath(path, sample, rayCount, transfer, skyVisibility, directEmission);
    if (r % 4 === 3) yield;
  }
  return rays;
}
