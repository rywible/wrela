import type { TerrainDefinition, Vec3 } from "@wrela/model";

import { compileField } from "./field";
import { buildGeologyObject } from "./geology-formations";
import { createTerrainSampler } from "./terrain";

export { createGeologyCorridor } from "./geology-corridors";
export {
  buildGeologyObject,
  buildGeologyObjects,
  geologyGeneratorId,
  geologyObjectId,
} from "./geology-formations";

export type GeologyRouteSample = {
  position: Vec3;
  slope: number;
  blocked: boolean;
  overheadClearance: boolean;
};
export type GeologyReview = {
  samples: GeologyRouteSample[];
  steepSamples: number;
  collisionSamples: number;
  maxSlope: number;
  sightlineClear: boolean;
  sightlineObstruction: Vec3 | null;
  sampleSpacing: number;
  bodyRadius: number;
  /** Diagnostics are sampled, not a physics certification. */
  approximate: true;
};
export function reviewGeology(terrain: TerrainDefinition): GeologyReview | undefined {
  if (!terrain.geology) return undefined;
  const { review, formations } = terrain.geology;
  const sampler = createTerrainSampler(terrain);
  const obstacles = formations.map((formation) => {
    const object = buildGeologyObject(terrain, formation);
    return { formation, bounds: object.field.bounds, field: compileField(object.field) };
  });
  const bodyRadius = review.bodyRadius ?? 0.35;
  const footprint: [number, number][] = [[0, 0]];
  if (bodyRadius > 0)
    for (let i = 0; i < 8; i++) {
      const angle = (i * Math.PI) / 4;
      footprint.push([Math.cos(angle) * bodyRadius, Math.sin(angle) * bodyRadius]);
    }
  const inside = (p: Vec3) =>
    obstacles.some(({ formation, bounds, field }) => {
      const local: Vec3 = [
        p[0] - formation.position[0],
        p[1] - formation.position[1],
        p[2] - formation.position[2],
      ];
      if (local.some((value, axis) => value < bounds.min[axis] || value > bounds.max[axis])) return false;
      return field.distance(local) < 0;
    });
  const lengths = review.route
    .slice(1)
    .map((p, i) => Math.hypot(p[0] - review.route[i][0], p[1] - review.route[i][1]));
  const total = lengths.reduce((sum, value) => sum + value, 0);
  const sampleCount = Math.max(2, Math.min(512, Math.ceil(total / 0.5) + 1));
  const samples: GeologyRouteSample[] = [];
  let segment = 0,
    traversed = 0;
  for (let i = 0; i < sampleCount; i++) {
    const distance = (i / (sampleCount - 1)) * total;
    while (segment < lengths.length - 1 && distance > traversed + lengths[segment])
      traversed += lengths[segment++];
    const a = review.route[segment],
      b = review.route[segment + 1];
    const t = lengths[segment] > 0 ? (distance - traversed) / lengths[segment] : 0;
    const x = a[0] + (b[0] - a[0]) * t,
      z = a[1] + (b[1] - a[1]) * t;
    const y = sampler.height(x, z);
    const slope = Math.max(
      ...footprint.map(
        ([dx, dz]) =>
          (Math.acos(Math.max(-1, Math.min(1, sampler.normal(x + dx, z + dz)[1]))) * 180) / Math.PI,
      ),
    );
    // A center and eight rim samples at five heights review the full body width.
    // It is bounded sampled evidence, not a continuous swept physics query.
    const body = footprint.flatMap(([dx, dz]) =>
      [0.075, 0.25, 0.5, 0.75, 1].map((fraction): Vec3 => [x + dx, y + review.clearance * fraction, z + dz]),
    );
    const blocked = body.some(inside);
    const overheadClearance = !footprint.some(([dx, dz]) => inside([x + dx, y + review.clearance, z + dz]));
    samples.push({ position: [x, y, z], slope, blocked, overheadClearance });
  }
  const first = samples[0].position,
    last = samples[samples.length - 1].position;
  let sightlineObstruction: Vec3 | null = null;
  const sightSamples = Math.max(
    2,
    Math.min(512, Math.ceil(Math.hypot(last[0] - first[0], last[2] - first[2]) / 0.5) + 1),
  );
  for (let i = 1; i < sightSamples - 1; i++) {
    const t = i / (sightSamples - 1);
    const p: Vec3 = [
      first[0] + (last[0] - first[0]) * t,
      first[1] + (last[1] - first[1]) * t + review.eyeHeight,
      first[2] + (last[2] - first[2]) * t,
    ];
    if (sampler.height(p[0], p[2]) > p[1] || inside(p)) {
      sightlineObstruction = p;
      break;
    }
  }
  return {
    samples,
    steepSamples: samples.filter((s) => s.slope > review.maxSlope).length,
    collisionSamples: samples.filter((s) => s.blocked).length,
    maxSlope: Math.max(...samples.map((s) => s.slope)),
    sightlineClear: !sightlineObstruction,
    sightlineObstruction,
    sampleSpacing: total / (sampleCount - 1),
    bodyRadius,
    approximate: true,
  };
}
