import { expect, test } from "bun:test";
import { buildGeologyObject, compileSurface, generateTerrainPatch, terrainHeight } from "@wrela/compiler";
import { defaultTerrainGeology, type TerrainDefinition } from "@wrela/model";

import { PhysicsAdapter } from "./physics";

function source(): TerrainDefinition {
  return {
    id: "ground",
    name: "Ground",
    schemaVersion: 1,
    kind: "terrain",
    dependencies: [],
    material: "rock",
    seed: 1,
    amplitude: 0,
    baseHeight: 0,
    frequency: 0.01,
    octaves: 2,
    interventions: [],
    geology: defaultTerrainGeology(),
  };
}
test("geological heightfield collision uses the rendered ridge surface", async () => {
  const terrain = source();
  if (!terrain.geology) throw new Error("Missing geology");
  terrain.geology.landforms = [
    {
      id: "ridge",
      kind: "ridge",
      points: [
        [-10, 0],
        [10, 0],
      ],
      width: 5,
      height: 6,
      falloff: 1,
    },
  ];
  const patch = generateTerrainPatch(terrain, -8, -8, 16, 32);
  const physics = await PhysicsAdapter.create();
  try {
    physics.installTerrain([{ mesh: patch, x: -8, z: -8 }]);
    physics.step(1 / 60);
    const hit = physics.raycast([0, 20, 0], [0, -1, 0], 30);
    expect(hit?.point[1]).toBeCloseTo(terrainHeight(terrain, 0, 0), 4);
  } finally {
    physics.dispose();
  }
});
test("published cave triangle collision leaves its tunnel open and roof solid", async () => {
  const object = buildGeologyObject(source(), {
    id: "tunnel",
    kind: "cave",
    position: [0, 0, 0],
    size: [12, 8, 10],
    opening: 0.65,
    resolution: 32,
  });
  const compiled = compileSurface(object, "review");
  const physics = await PhysicsAdapter.create();
  try {
    physics.addStaticMesh(object.id, compiled.mesh, [0, 0, 0]);
    physics.step(1 / 60);
    expect(physics.raycast([0, 1.5, -10], [0, 0, 1], 20)).toBeNull();
    expect(physics.raycast([5, 1.5, -10], [0, 0, 1], 20)?.id).toBe(object.id);
    expect(physics.raycast([0, 1.5, 0], [0, 1, 0], 10)?.id).toBe(object.id);
  } finally {
    physics.dispose();
  }
});

test("protected corridor collision remains walkable through a resculpted ridge", async () => {
  const terrain = source();
  if (!terrain.geology) throw new Error("Missing geology");
  terrain.geology.landforms = [
    {
      id: "ridge",
      kind: "ridge",
      points: [
        [-10, 0],
        [10, 0],
      ],
      width: 5,
      height: 12,
      falloff: 1,
    },
  ];
  terrain.geology.corridors = [
    {
      id: "trail",
      points: [
        [0, 1, -8],
        [0, 1, 8],
      ],
      halfWidth: 1.5,
      shoulder: 2,
    },
  ];
  const physics = await PhysicsAdapter.create();
  try {
    physics.installTerrain([{ mesh: generateTerrainPatch(terrain, -8, -8, 16, 32), x: -8, z: -8 }]);
    physics.step(1 / 60);
    for (const z of [-6, -2, 0, 2, 6])
      expect(physics.raycast([0.5, 20, z], [0, -1, 0], 30)?.point[1]).toBeCloseTo(1, 4);
    expect(physics.raycast([4, 20, 0], [0, -1, 0], 30)?.point[1]).toBeCloseTo(12, 4);
  } finally {
    physics.dispose();
  }
});

test("stratified rotated cave collision follows its opening and preserves its roof", async () => {
  const object = buildGeologyObject(source(), {
    id: "cleaved-cave",
    kind: "cave",
    position: [0, 0, 0],
    size: [12, 8, 10],
    opening: 0.65,
    resolution: 40,
    heading: Math.PI / 2,
    rock: { seed: 17, layers: 5, fracture: 0.8 },
  });
  const compiled = compileSurface(object, "review");
  const physics = await PhysicsAdapter.create();
  try {
    physics.addStaticMesh(object.id, compiled.mesh, [0, 0, 0]);
    physics.step(1 / 60);
    expect(physics.raycast([-10, 1.5, 0], [1, 0, 0], 20)).toBeNull();
    expect(physics.raycast([-10, 1.5, 5], [1, 0, 0], 20)?.id).toBe(object.id);
    expect(physics.raycast([0, 1.5, 0], [0, 1, 0], 10)?.id).toBe(object.id);
  } finally {
    physics.dispose();
  }
});
