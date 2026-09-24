import { expect, test } from "bun:test";
import { compileDocument, createTerrainSampler, generateTerrainPatch } from "@wrela/compiler";
import { sampleWorldPolyline, type Vec3, worldPathPolyline } from "@wrela/model";

import {
  AssemblyMotion,
  assemblyWorldMatrix,
  installObjectCollision,
  PhysicsAdapter,
  quatFromEuler,
} from "@wrela/runtime";
import { realizeWorldComposition } from "@wrela/world";
import { createAlpineSliceStudy } from "./alpine-slice-study";

/** Real source meshes, runtime assembly proxies and triangle terrain, without a GPU or renderer. */
test("river-bend route reaches and enters the shelter through its open side", async () => {
  const { project } = createAlpineSliceStudy("river-bend", "portable");
  const world = project.documents.find((document) => document.id === project.entry);
  if (world?.kind !== "world" || !world.composition) throw new Error("Missing review world");
  const terrain = project.documents.find((document) => document.id === world.terrain);
  if (terrain?.kind !== "terrain") throw new Error("Missing review terrain");
  const realized = realizeWorldComposition(world, terrain);
  const shelter = realized.world.instances.find(
    (instance) => instance.definition === "alpine-lookdev-gateway",
  );
  if (!shelter) throw new Error("Missing grounded shelter instance");
  const matrix = assemblyWorldMatrix(shelter.position, shelter.rotation, shelter.scale);
  const transform = ([x, y, z]: Vec3): Vec3 => [
    matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12],
    matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13],
    matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14],
  ];
  const physics = await PhysicsAdapter.create();
  try {
    physics.installTerrain([
      { mesh: generateTerrainPatch(realized.terrain, -24, -24, 48, 192), x: -24, z: -24 },
    ]);
    for (const instance of realized.world.instances) {
      const definition = project.documents.find((document) => document.id === instance.definition);
      if (definition?.kind !== "object" || definition.collision === "none") continue;
      const artifact = compileDocument(definition, "review");
      if (artifact?.kind !== "surface") throw new Error("Missing compiled collision source");
      if (definition.assembly)
        new AssemblyMotion(definition.assembly, artifact.mesh).installCollision(
          physics,
          instance.id,
          assemblyWorldMatrix(instance.position, instance.rotation, instance.scale),
          0,
        );
      else
        installObjectCollision(
          physics,
          instance.id,
          definition,
          artifact.mesh,
          instance.position,
          quatFromEuler(instance.rotation),
          instance.scale,
        );
    }
    physics.step(1 / 60);
    const route = world.composition.paths.flatMap((path, index) => {
      const points = worldPathPolyline(path);
      return index ? points.slice(1) : points;
    });
    // Continue from the authored endpoint into the reserved entry, proving it is an opening rather than a wall-facing endpoint.
    const entry = transform([0.315, 0, 1.95]);
    const points = [...route, entry];
    const ground = createTerrainSampler(realized.terrain);
    const blocked: { position: Vec3; ids: string[] }[] = [];
    for (const { position } of sampleWorldPolyline(points, 241)) {
      const height = ground.height(position[0], position[2]);
      const hit = physics.raycast([position[0], height + 0.25, position[2]], [0, -1, 0], 0.7);
      expect(hit).not.toBeNull();
      const level = hit?.point[1] ?? height;
      const ids = [
        ...new Set(
          [0.67, 0.9, 1.45].flatMap((offset) =>
            physics.overlapSphere([position[0], level + offset, position[2]], 0.35),
          ),
        ),
      ];
      if (ids.length) blocked.push({ position, ids });
    }
    expect(blocked).toEqual([]);
    const inside = transform([0.315, 0.4, 1.95]);
    const wallHit = physics.raycast(inside, [-matrix[8], -matrix[9], -matrix[10]], 3);
    expect(wallHit?.id).toBe(shelter.id);
    const endpoint = route.at(-1);
    if (!endpoint) throw new Error("Missing route endpoint");
    expect(Math.hypot(entry[0] - endpoint[0], entry[2] - endpoint[2])).toBeLessThan(1.5);
  } finally {
    physics.dispose();
  }
}, 20000);
