import { compileAssemblyMesh, terrainHeight, terrainNormal } from "@wrela/compiler";
import {
  type Bounds,
  type Document,
  emptyWorldComposition,
  sampleWorldPolyline,
  type TerrainDefinition,
  type Vec3,
  type WorldComposition,
  type WorldDefinition,
  worldPathLength,
  worldPathPolyline,
} from "@wrela/model";

import { realizeWorldComposition } from "./composition";
import { reviewRouteNetwork, type WorldNetworkReview } from "./route-network";

export type { WorldNetworkReview } from "./route-network";

export type WorldObstacle = { id: string; bounds: Bounds; collision: boolean };
export type WorldRouteSample = {
  position: Vec3;
  blockedBy: string[];
  grade: number;
  edgeStep: number;
  traversable: boolean;
};
export type WorldSpatialReview = {
  approximation: "sampled-terrain-conservative-instance-bounds";
  routes: {
    id: string;
    samples: WorldRouteSample[];
    sampleSpacing: number;
    blockedSamples: number;
    steepSamples: number;
    narrow: boolean;
  }[];
  sightline: {
    from: Vec3;
    to: Vec3;
    clear: boolean;
    obstruction?: { id: string; position: Vec3 };
    terrainSampleSpacing: number;
  };
  obstacles: WorldObstacle[];
  network: WorldNetworkReview;
};
const defaultReview = {
  actorRadius: 0.35,
  actorHeight: 1.8,
  maxStepHeight: 0.3,
  sightline: { from: [0, 1.7, 0] as Vec3, to: [10, 1.7, 0] as Vec3 },
};
export function defaultWorldReview(): NonNullable<WorldComposition["review"]> {
  return structuredClone(defaultReview);
}

/** Rotate every source-envelope corner; bounds remain conservative for hollow objects. */
function instanceBounds(bounds: Bounds, instance: WorldDefinition["instances"][number]): Bounds {
  const min: Vec3 = [Infinity, Infinity, Infinity],
    max: Vec3 = [-Infinity, -Infinity, -Infinity];
  const [rx, ry, rz] = instance.rotation;
  // XYZ intrinsic Euler, matching the runtime quaternion convention.
  const qx =
    Math.sin(rx / 2) * Math.cos(ry / 2) * Math.cos(rz / 2) +
    Math.cos(rx / 2) * Math.sin(ry / 2) * Math.sin(rz / 2);
  const qy =
    Math.cos(rx / 2) * Math.sin(ry / 2) * Math.cos(rz / 2) -
    Math.sin(rx / 2) * Math.cos(ry / 2) * Math.sin(rz / 2);
  const qz =
    Math.cos(rx / 2) * Math.cos(ry / 2) * Math.sin(rz / 2) +
    Math.sin(rx / 2) * Math.sin(ry / 2) * Math.cos(rz / 2);
  const qw =
    Math.cos(rx / 2) * Math.cos(ry / 2) * Math.cos(rz / 2) -
    Math.sin(rx / 2) * Math.sin(ry / 2) * Math.sin(rz / 2);
  for (let corner = 0; corner < 8; corner++) {
    const p: Vec3 = [0, 1, 2].map(
      (axis) => (corner & (1 << axis) ? bounds.max[axis] : bounds.min[axis]) * instance.scale,
    ) as Vec3;
    const tx = 2 * (qy * p[2] - qz * p[1]),
      ty = 2 * (qz * p[0] - qx * p[2]),
      tz = 2 * (qx * p[1] - qy * p[0]);
    const v: Vec3 = [
      p[0] + qw * tx + qy * tz - qz * ty,
      p[1] + qw * ty + qz * tx - qx * tz,
      p[2] + qw * tz + qx * ty - qy * tx,
    ];
    for (let axis = 0; axis < 3; axis++) {
      min[axis] = Math.min(min[axis], v[axis] + instance.position[axis]);
      max[axis] = Math.max(max[axis], v[axis] + instance.position[axis]);
    }
  }
  return { min, max };
}
function segmentBox(from: Vec3, to: Vec3, bounds: Bounds): number | undefined {
  let near = 0,
    far = 1;
  for (let axis = 0; axis < 3; axis++) {
    const d = to[axis] - from[axis];
    if (Math.abs(d) < 1e-9) {
      if (from[axis] < bounds.min[axis] || from[axis] > bounds.max[axis]) return;
      continue;
    }
    const a = (bounds.min[axis] - from[axis]) / d,
      b = (bounds.max[axis] - from[axis]) / d;
    near = Math.max(near, Math.min(a, b));
    far = Math.min(far, Math.max(a, b));
    if (near > far) return;
  }
  return near;
}
const interpolate = (a: Vec3, b: Vec3, t: number): Vec3 => [
  a[0] + t * (b[0] - a[0]),
  a[1] + t * (b[1] - a[1]),
  a[2] + t * (b[2] - a[2]),
];
/** Review source worlds, not previously realized worlds. Never claims navmesh reachability. */
export function reviewWorldComposition(
  source: WorldDefinition,
  terrainSource: TerrainDefinition,
  documents: readonly Document[],
): WorldSpatialReview {
  const { world, terrain } = realizeWorldComposition(source, terrainSource);
  const settings = source.composition?.review ?? defaultReview;
  const definitions = new Map(documents.map((document) => [document.id, document]));
  const assemblyPartBounds = new Map<string, { id: string; bounds: Bounds; collision: boolean }[]>();
  const obstacles: WorldObstacle[] = world.instances.flatMap((instance) => {
    const definition = definitions.get(instance.definition);
    if (!definition) return [];
    if (definition.kind === "object" && definition.assembly) {
      let parts = assemblyPartBounds.get(definition.id);
      if (!parts) {
        const mesh = compileAssemblyMesh(definition.assembly, definition.material).mesh;
        const bounds = new Map<string, Bounds>();
        for (let vertex = 0; vertex < mesh.positions.length / 3; vertex++) {
          const id = mesh.sourceIds?.[vertex];
          if (!id) continue;
          const part = bounds.get(id) ?? {
            min: [Infinity, Infinity, Infinity] as Vec3,
            max: [-Infinity, -Infinity, -Infinity] as Vec3,
          };
          for (let axis = 0; axis < 3; axis++) {
            const coordinate = mesh.positions[vertex * 3 + axis];
            part.min[axis] = Math.min(part.min[axis], coordinate);
            part.max[axis] = Math.max(part.max[axis], coordinate);
          }
          bounds.set(id, part);
        }
        parts = definition.assembly.parts.flatMap((part) => {
          const partBounds = bounds.get(part.id);
          return partBounds ? [{ id: part.id, bounds: partBounds, collision: part.collision !== false }] : [];
        });
        assemblyPartBounds.set(definition.id, parts);
      }
      return parts.map((part) => ({
        id: `${instance.id}/${part.id}`,
        bounds: instanceBounds(part.bounds, instance),
        collision: definition.collision !== "none" && part.collision,
      }));
    }
    const bounds: Bounds | undefined =
      "field" in definition
        ? definition.field.bounds
        : definition.kind === "vegetation"
          ? {
              min: [-definition.radius, 0, -definition.radius],
              max: [definition.radius, definition.height, definition.radius],
            }
          : undefined;
    return bounds
      ? [
          {
            id: instance.id,
            bounds: instanceBounds(bounds, instance),
            collision:
              definition.kind === "object"
                ? definition.collision !== "none"
                : definition.kind === "character",
          },
        ]
      : [];
  });
  const sampleAt = (point: Vec3, maxGrade: number): WorldRouteSample => {
    const position: Vec3 = [...point];
    position[1] = terrainHeight(terrain, position[0], position[2]);
    const normal = terrainNormal(terrain, position[0], position[2]);
    const grade = Math.sqrt(Math.max(0, 1 - normal[1] * normal[1])) / Math.max(1e-6, normal[1]);
    const edgeStep = Math.max(
      ...[
        [settings.actorRadius, 0],
        [-settings.actorRadius, 0],
        [0, settings.actorRadius],
        [0, -settings.actorRadius],
      ].map(([x, z]) => Math.abs(terrainHeight(terrain, position[0] + x, position[2] + z) - position[1])),
    );
    const blockedBy = obstacles
      .filter(
        ({ bounds, collision }) =>
          collision &&
          bounds.max[1] > position[1] + 0.05 &&
          bounds.min[1] < position[1] + settings.actorHeight &&
          Math.hypot(
            Math.max(bounds.min[0] - position[0], 0, position[0] - bounds.max[0]),
            Math.max(bounds.min[2] - position[2], 0, position[2] - bounds.max[2]),
          ) < settings.actorRadius,
      )
      .map((obstacle) => obstacle.id);
    return {
      position,
      grade,
      edgeStep,
      blockedBy,
      traversable: !blockedBy.length && grade <= maxGrade && edgeStep <= settings.maxStepHeight,
    };
  };
  const routes = (source.composition?.paths ?? []).map((path) => {
    const points = worldPathPolyline(path);
    const total = worldPathLength(points);
    const count = Math.max(2, Math.min(512, Math.ceil(total / Math.min(0.5, settings.actorRadius)) + 1));
    const samples = sampleWorldPolyline(points, count).map(({ position }) => {
      const sample = sampleAt(position, path.maxGrade);
      sample.traversable &&= path.width >= settings.actorRadius * 2;
      return sample;
    });
    return {
      id: path.id,
      samples,
      sampleSpacing: total / (count - 1),
      blockedSamples: samples.filter((sample) => !sample.traversable).length,
      steepSamples: samples.filter((sample) => sample.grade > path.maxGrade).length,
      narrow: path.width < settings.actorRadius * 2,
    };
  });
  const { from, to } = settings.sightline;
  let hit = Infinity,
    obstruction: { id: string; position: Vec3 } | undefined;
  for (const obstacle of obstacles) {
    // The authored endpoint may deliberately identify the subject being reviewed.
    if (
      to.every(
        (coordinate, axis) =>
          coordinate >= obstacle.bounds.min[axis] && coordinate <= obstacle.bounds.max[axis],
      )
    )
      continue;
    const t = segmentBox(from, to, obstacle.bounds);
    if (t !== undefined && t < hit) {
      hit = t;
      obstruction = { id: obstacle.id, position: interpolate(from, to, t) };
    }
  }
  const length = Math.hypot(to[0] - from[0], to[1] - from[1], to[2] - from[2]),
    count = Math.max(2, Math.min(512, Math.ceil(length / 0.25) + 1));
  for (let index = 0; index < count; index++) {
    const t = index / (count - 1);
    if (t >= hit) break;
    const p = interpolate(from, to, t);
    if (terrainHeight(terrain, p[0], p[2]) > p[1]) {
      obstruction = { id: terrain.id, position: p };
      break;
    }
  }
  return {
    approximation: "sampled-terrain-conservative-instance-bounds",
    routes,
    obstacles,
    network: reviewRouteNetwork(
      source.composition ?? emptyWorldComposition(),
      routes,
      settings.actorRadius,
      (position) =>
        sampleAt(position, Math.max(0.25, ...(source.composition?.paths ?? []).map((path) => path.maxGrade)))
          .traversable,
    ),
    sightline: { from, to, clear: !obstruction, obstruction, terrainSampleSpacing: length / (count - 1) },
  };
}
