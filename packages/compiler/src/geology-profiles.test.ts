import { expect, test } from "bun:test";
import { defaultTerrainGeology, documentSchema, type TerrainDefinition } from "@wrela/model";

import { compileGeologicalLandforms, geologicalLandformHeight } from "./geology-height";
import { createTerrainSampler, generateTerrainPatch, terrainHeight } from "./terrain";

function study(): TerrainDefinition {
  const geology = defaultTerrainGeology();
  geology.landforms = [
    {
      id: "river",
      kind: "drainage",
      points: [
        [0, -30],
        [0, 0],
        [5, 15],
        [-3, 30],
      ],
      width: 10,
      height: 2,
      falloff: 1,
      profile: { kind: "river", bedFraction: 0.2, bankFraction: 0.45, shoulderDepth: 0.15, asymmetry: 0.4 },
    },
  ];
  return {
    id: "profile-ground",
    name: "Profile ground",
    schemaVersion: 1,
    dependencies: [],
    kind: "terrain",
    material: "ground",
    seed: 42,
    amplitude: 0,
    frequency: 0.01,
    octaves: 2,
    baseHeight: 0,
    interventions: [],
    geology,
  };
}

test("river profile separates channel, asymmetric banks and broad shoulders with continuous normals", () => {
  const terrain = study();
  expect(documentSchema.safeParse(terrain).success).toBe(true);
  expect(terrainHeight(terrain, 0, -15)).toBe(-2);
  expect(terrainHeight(terrain, 1, -15)).toBe(-2);
  expect(terrainHeight(terrain, -4, -15)).not.toBe(terrainHeight(terrain, 4, -15));
  expect(terrainHeight(terrain, 8, -15)).toBeLessThan(0);
  expect(terrainHeight(terrain, 10, -15)).toBe(0);
  const epsilon = 1e-4;
  // Every source sample and first derivative remains continuous across bed,
  // bank and support boundaries, including both asymmetric banks.
  for (let x = -10; x <= 10; x += 0.05) {
    const slopeBefore = (terrainHeight(terrain, x, -15) - terrainHeight(terrain, x - epsilon, -15)) / epsilon;
    const slopeAfter = (terrainHeight(terrain, x + epsilon, -15) - terrainHeight(terrain, x, -15)) / epsilon;
    expect(Math.abs(slopeBefore - slopeAfter)).toBeLessThan(0.001);
  }
});

test("profiled cliff has a talus toe, directed face and crest, without support jumps", () => {
  const terrain = study();
  if (!terrain.geology) throw Error("Missing geology");
  terrain.geology.landforms = [
    {
      id: "cliff",
      kind: "cliff",
      points: [
        [-20, 0],
        [20, 0],
      ],
      width: 10,
      height: 8,
      falloff: 1,
      profile: { kind: "cliff", faceFraction: 0.12, toeHeight: 0.2, crestFraction: 0.7 },
    },
  ];
  expect(terrainHeight(terrain, 0, -10)).toBe(0);
  expect(terrainHeight(terrain, 0, -5)).toBeGreaterThan(0);
  expect(terrainHeight(terrain, 0, -5)).toBeLessThan(1.6);
  expect(terrainHeight(terrain, 0, 3)).toBe(8);
  expect(terrainHeight(terrain, 0, 7)).toBe(8);
  expect(terrainHeight(terrain, 0, 10)).toBe(0);
  terrain.geology.landforms[0].kind = "ridge";
  expect(documentSchema.safeParse(terrain).success).toBe(false);
});

test("profile spatial pruning and snapshots preserve direct sampling, patch seams and protected grades", () => {
  const terrain = study();
  if (!terrain.geology) throw Error("Missing geology");
  const profile = terrain.geology.landforms[0].profile;
  if (!profile) throw Error("Missing profile");
  profile.variation = { seed: 719, amplitude: 0.25, wavelength: 9 };
  const compiled = compileGeologicalLandforms(terrain.geology);
  for (let z = -40; z <= 40; z += 1.25)
    for (let x = -18; x <= 18; x += 1.75)
      expect(compiled(0, x, z)).toBe(geologicalLandformHeight(terrain.geology, 0, x, z));
  const before = compiled(0, 5, -12);
  profile.variation.seed = 42;
  expect(compiled(0, 5, -12)).toBe(before);
  terrain.geology.corridors = [
    {
      id: "ford",
      halfWidth: 1.2,
      shoulder: 2,
      points: [
        [-15, 0.3, -10],
        [15, 0.3, -10],
      ],
    },
  ];
  const sample = createTerrainSampler(terrain);
  for (let x = -12; x <= 12; x += 0.25) expect(sample.height(x, -10)).toBeCloseTo(0.3, 12);
  const left = generateTerrainPatch(terrain, -10, -20, 10, 16),
    right = generateTerrainPatch(terrain, 0, -20, 10, 16);
  for (let row = 0; row <= 16; row++) {
    expect(left.positions[(row * 17 + 16) * 3 + 1]).toBe(right.positions[row * 17 * 3 + 1]);
    expect([...left.normals.slice((row * 17 + 16) * 3, (row * 17 + 16) * 3 + 3)]).toEqual([
      ...right.normals.slice(row * 17 * 3, row * 17 * 3 + 3),
    ]);
  }
});
