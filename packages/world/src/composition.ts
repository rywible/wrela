import { createTerrainSampler } from "@wrela/compiler";
import {
  type CompositionIssue,
  sampleWorldPath,
  type TerrainDefinition,
  type Vec3,
  validateWorldComposition,
  type WorldComposition,
  type WorldDefinition,
  worldPathPolyline,
  worldRoomModules,
} from "@wrela/model";

import type { InterestSource } from "./planner";

function transform(point: Vec3, origin: Vec3, yaw: number, scale = 1): Vec3 {
  const c = Math.cos(yaw),
    s = Math.sin(yaw);
  return [
    origin[0] + scale * (c * point[0] + s * point[2]),
    origin[1] + scale * point[1],
    origin[2] + scale * (-s * point[0] + c * point[2]),
  ];
}
export function distanceToPath(points: readonly Vec3[], x: number, z: number): number {
  let distance = Infinity;
  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1],
      b = points[i],
      dx = b[0] - a[0],
      dz = b[2] - a[2];
    const t = Math.max(0, Math.min(1, ((x - a[0]) * dx + (z - a[2]) * dz) / (dx * dx + dz * dz || 1)));
    distance = Math.min(distance, Math.hypot(x - a[0] - t * dx, z - a[2] - t * dz));
  }
  return distance;
}
/** Biomes affect only their named rules; authored roads and spaces remain clear. */
export function compositionPopulationDensity(
  composition: WorldComposition | undefined,
  rule: string,
  x: number,
  z: number,
): number {
  return createCompositionPopulationSampler(composition)(rule, x, z);
}
/** Prepare curve geometry once per population batch, not once per canonical cell. */
export function createCompositionPopulationSampler(composition: WorldComposition | undefined) {
  const paths = (composition?.paths ?? []).map((path) => ({ ...path, points: worldPathPolyline(path) }));
  return (rule: string, x: number, z: number): number => {
    if (!composition) return 1;
    if (paths.some((path) => distanceToPath(path.points, x, z) < path.width / 2 + path.shoulder)) return 0;
    if (
      composition.spaces.some(
        (space) =>
          space.clearPopulation && Math.hypot(x - space.center[0], z - space.center[2]) < space.radius,
      )
    )
      return 0;
    if (
      composition.rooms.some((room) => {
        const local = transform([x - room.center[0], 0, z - room.center[2]], [0, 0, 0], -room.yaw);
        return Math.abs(local[0]) <= room.size[0] / 2 && Math.abs(local[2]) <= room.size[1] / 2;
      })
    )
      return 0;
    const biomes = composition.biomes.filter((biome) => biome.populations.includes(rule));
    if (!biomes.length) return 1;
    return Math.max(
      0,
      ...biomes.map((biome) => {
        const d = Math.hypot(x - biome.center[0], z - biome.center[2]);
        const t =
          biome.transition === 0
            ? Number(d < biome.radius)
            : Math.max(0, Math.min(1, (biome.radius - d) / biome.transition));
        return biome.density * t * t * (3 - 2 * t);
      }),
    );
  };
}
/** Regions request residency only while the observer is near, avoiding global pinned terrain. */
export function compositionInterests(
  composition: WorldComposition | undefined,
  observer: Vec3,
): InterestSource[] {
  return (composition?.streaming ?? [])
    .filter(
      (region) =>
        Math.hypot(observer[0] - region.center[0], observer[2] - region.center[2]) <=
        region.radius + region.preloadDistance,
    )
    .map((region) => ({
      id: `composition:${region.id}`,
      position: region.center,
      visualRadius: region.radius,
      collisionRadius: region.collision ? region.radius : 0,
      priority: region.priority,
    }));
}
export function realizeWorldComposition(
  world: WorldDefinition,
  terrain: TerrainDefinition,
): { world: WorldDefinition; terrain: TerrainDefinition; diagnostics: CompositionIssue[] } {
  const composition = world.composition;
  if (!composition)
    return {
      world: { ...world, instances: groundInstances(world.instances, terrain) },
      terrain,
      diagnostics: [],
    };
  const diagnostics = validateWorldComposition(
    composition,
    world.populations.map((rule) => rule.id),
    {
      instanceIds: world.instances.map((instance) => instance.id),
      interventionCount: terrain.interventions.length,
    },
  );
  if (diagnostics.some((issue) => issue.severity === "error"))
    throw new Error(diagnostics.map((issue) => issue.message).join("; "));
  const overrides = new Map(composition.overrides.map((override) => [override.id, override]));
  const instances = structuredClone(world.instances).flatMap((instance) => {
      const override = overrides.get(instance.id);
      return override?.removed
        ? []
        : [
            {
              ...instance,
              position: override?.position ?? instance.position,
              rotation: override?.yaw === undefined ? instance.rotation : ([0, override.yaw, 0] as Vec3),
              scale: override?.scale ?? instance.scale,
            },
          ];
    }),
    interventions = structuredClone(terrain.interventions);
  const prefix = "layout_";
  const existing = new Set(instances.map((instance) => instance.id));
  const emit = (id: string, definition: string, position: Vec3, yaw: number, scale = 1) => {
    const generated = `${prefix}${id}`;
    if (existing.has(generated)) throw new Error(`Composition instance identity collides: ${generated}`);
    existing.add(generated);
    const override = overrides.get(generated);
    if (override?.removed) return;
    if (instances.length >= 256)
      throw new Error("World composition exceeds 256 instances; split the layout into smaller scenes");
    instances.push({
      id: generated,
      definition,
      position: override?.position ?? position,
      rotation: [0, override?.yaw ?? yaw, 0],
      scale: override?.scale ?? scale,
    });
  };
  // Finish grading before grounding reusable groups so every member retains its
  // local elevation relative to the group's grounded anchor.
  for (const path of composition.paths) {
    if (!path.flatten) continue;
    for (const [index, { position }] of sampleWorldPath(path, path.width / 2).entries()) {
      const id = `${prefix}path_${path.id}_${index}`;
      if (interventions.length >= 256) throw new Error("World paths exceed the terrain intervention budget");
      if (interventions.some((edit) => edit.id === id))
        throw new Error(`Composition intervention identity collides: ${id}`);
      interventions.push({
        id,
        kind: "flatten",
        center: [position[0], position[2]],
        radius: Math.max(1, path.width / 2 + path.shoulder),
        strength: 1,
        targetHeight: position[1],
      });
    }
  }
  const realizedTerrain = { ...terrain, interventions };
  let sampler: ReturnType<typeof createTerrainSampler> | undefined;
  const groundedOrigin = (position: Vec3, grounding?: { offset: number }): Vec3 => {
    if (!grounding) return position;
    sampler ??= createTerrainSampler(realizedTerrain);
    return [position[0], sampler.height(position[0], position[2]) + grounding.offset, position[2]];
  };
  const assemblies = new Map(composition.assemblies.map((assembly) => [assembly.id, assembly]));
  for (const placement of composition.placements) {
    const origin = groundedOrigin(placement.position, placement.grounding);
    for (const member of assemblies.get(placement.assembly)?.members ?? []) {
      emit(
        `assembly_${placement.id}_${member.id}`,
        member.definition,
        transform(member.position, origin, placement.yaw, placement.scale),
        placement.yaw + member.yaw,
        placement.scale * member.scale,
      );
    }
  }
  for (const space of composition.spaces) {
    const origin = groundedOrigin(space.center, space.grounding);
    for (const member of space.members)
      emit(
        `space_${space.id}_${member.id}`,
        member.definition,
        transform(member.position, origin, 0),
        member.yaw,
        member.scale,
      );
  }
  for (const room of composition.rooms)
    for (const module of worldRoomModules(room))
      emit(
        `room_${room.id}_${module.side}_${module.index}`,
        room.wallDefinition,
        transform(module.position, room.center, room.yaw),
        room.yaw + module.yaw,
      );
  for (const path of composition.paths) {
    if (!path.definition) continue;
    for (const [index, { position, yaw }] of sampleWorldPath(path, path.spacing).entries())
      emit(`path_${path.id}_${index}`, path.definition, position, yaw);
  }
  return {
    world: { ...world, instances: groundInstances(instances, realizedTerrain, overrides) },
    terrain: realizedTerrain,
    diagnostics,
  };
}

/** Terrain following is resolved after authored path grading. An explicit placement
 * exception owns its elevation, while ordinary source edits remain grounded. */
function groundInstances(
  instances: readonly WorldDefinition["instances"][number][],
  terrain: TerrainDefinition,
  overrides: ReadonlyMap<string, WorldComposition["overrides"][number]> = new Map(),
): WorldDefinition["instances"] {
  if (!instances.some((instance) => instance.grounding && !overrides.get(instance.id)?.position))
    return [...instances];
  const sampler = createTerrainSampler(terrain);
  return instances.map((instance) => {
    if (!instance.grounding || overrides.get(instance.id)?.position) return instance;
    const [x, , z] = instance.position;
    return {
      ...instance,
      position: [x, sampler.height(x, z) + instance.grounding.offset, z],
    };
  });
}
