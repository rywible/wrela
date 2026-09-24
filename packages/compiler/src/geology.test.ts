import { expect, test } from "bun:test";
import {
  defaultTerrainGeology,
  documentSchema,
  type GeologyFormation,
  type TerrainDefinition,
  type TerrainGeology,
  terrainGeologySchema,
} from "@wrela/model";

import { compileField } from "./field";
import { buildGeologyObject, createGeologyCorridor, reviewGeology } from "./geology";
import { extractSurface } from "./surface";
import { createTerrainSampler, generateTerrainPatch, terrainHeight, terrainNormal } from "./terrain";

const terrain = (): TerrainDefinition & { geology: TerrainGeology } => ({
  id: "ground",
  name: "Ground",
  schemaVersion: 1,
  dependencies: [],
  kind: "terrain",
  seed: 1,
  amplitude: 0,
  frequency: 0.01,
  octaves: 2,
  baseHeight: 0,
  material: "rock",
  interventions: [],
  geology: defaultTerrainGeology(),
});
const cave: GeologyFormation = {
  id: "cave",
  kind: "cave",
  position: [0, 0, 0],
  size: [12, 8, 10],
  opening: 0.65,
  resolution: 32,
};

test("geology rejects unbounded and duplicate feature input", () => {
  const source = defaultTerrainGeology();
  source.formations = [cave, cave];
  expect(terrainGeologySchema.safeParse(source).success).toBe(false);
  source.formations = [{ ...cave, resolution: 256 }];
  expect(terrainGeologySchema.safeParse(source).success).toBe(false);
  source.formations = [{ ...cave, opening: Number.NaN }];
  expect(terrainGeologySchema.safeParse(source).success).toBe(false);
});

test("ridge, drainage and cliff geometry changes the actual sampled terrain", () => {
  const source = terrain();
  source.geology.landforms = [
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
  expect(terrainHeight(source, 0, 0)).toBeCloseTo(6);
  expect(terrainHeight(source, 0, 6)).toBe(0);
  expect(terrainNormal(source, 0, 2)[1]).toBeLessThan(0.8);
  source.geology.landforms[0].kind = "drainage";
  expect(terrainHeight(source, 0, 0)).toBeCloseTo(-6);
  source.geology.landforms[0].kind = "cliff";
  expect(terrainHeight(source, 0, 0.3)).toBeGreaterThan(terrainHeight(source, 0, -0.3));
});

test("local interventions preserve traversal edits after geological filters", () => {
  const source = terrain();
  source.geology.landforms = [
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
  source.geology.erosion.strength = 1;
  source.geology.strata.strength = 1;
  source.interventions = [
    { id: "path", kind: "flatten", center: [0, 0], radius: 3, strength: 1, targetHeight: 1.23 },
  ];
  expect(terrainHeight(source, 0, 0)).toBeCloseTo(1.23, 12);
});

test("erosion softens peaks and strata change surfaces without patch-order dependence", () => {
  const source = terrain();
  source.geology.landforms = [
    {
      id: "ridge",
      kind: "ridge",
      points: [
        [-10, 0],
        [10, 0],
      ],
      width: 3,
      height: 8,
      falloff: 1,
    },
  ];
  source.geology.erosion = { strength: 1, radius: 2, talusAngle: 5 };
  expect(terrainHeight(source, 0, 0)).toBeLessThan(8);
  source.geology.strata = { thickness: 3, strength: 0.8 };
  const right = generateTerrainPatch(source, 0, -4, 8, 8);
  const left = generateTerrainPatch(source, -8, -4, 8, 8);
  for (let row = 0; row <= 8; row++) {
    const a = (row * 9 + 8) * 3,
      b = row * 9 * 3;
    expect(left.positions[a + 1]).toBe(right.positions[b + 1]);
    expect([...left.normals.slice(a, a + 3)]).toEqual([...right.normals.slice(b, b + 3)]);
    expect(left.positions[a + 1]).toBeCloseTo(terrainHeight(source, 0, -4 + row), 5);
  }
  source.amplitude = 0;
  source.geology.landforms = [];
  source.baseHeight = 0.6;
  expect(terrainHeight(source, 0, 0)).not.toBe(0.6);
});

test("disabled geological filters preserve legacy terrain exactly", () => {
  const source = terrain();
  source.amplitude = 12;
  const legacy = { ...source, geology: undefined };
  for (const [x, z] of [
    [0, 0],
    [128.1, -60],
    [-17, 401],
  ])
    expect(terrainHeight(source, x, z)).toBe(terrainHeight(legacy, x, z));
});

test("wet drainage banks tint terrain without changing height, collision, or patch seams", () => {
  const source = terrain();
  source.geology.landforms = [
    {
      id: "brook",
      kind: "drainage",
      points: [
        [0, -10],
        [0, 10],
      ],
      width: 3,
      height: 1,
      falloff: 1,
    },
  ];
  source.geology.bankWetness = {
    drainageId: "brook",
    waterHalfWidth: 1,
    fadeWidth: 3,
    darkening: 0.4,
  };
  expect(documentSchema.safeParse(source).success).toBe(true);
  const height = terrainHeight(source, 2, 0);
  const near = generateTerrainPatch(source, -4, -4, 8, 8);
  const far = generateTerrainPatch(source, 4, -4, 8, 8);
  expect(terrainHeight(source, 2, 0)).toBe(height);
  expect(near.colors).toBeDefined();
  if (!near.colors || !far.colors) throw new Error("Expected drainage bank colors");
  expect(near.colors[(4 * 9 + 4) * 3]).toBeLessThan(near.colors[(4 * 9 + 8) * 3]);
  for (let row = 0; row <= 8; row++)
    expect([...near.colors.slice((row * 9 + 8) * 3, (row * 9 + 8) * 3 + 3)]).toEqual([
      ...far.colors.slice(row * 9 * 3, row * 9 * 3 + 3),
    ]);
  source.geology.bankWetness.drainageId = "missing";
  expect(documentSchema.safeParse(source).success).toBe(false);
});

test("finite caves compile to real cavity meshes with conservative solid walls", () => {
  const object = buildGeologyObject(terrain(), cave);
  expect(documentSchema.safeParse(object).success).toBe(true);
  expect(object.collision).toBe("mesh");
  const field = compileField(object.field);
  expect(field.distance([0, 1.5, 0])).toBeGreaterThan(0);
  expect(field.distance([0, 1.5, -5])).toBeGreaterThan(0);
  expect(field.distance([0, 1.5, 5])).toBeGreaterThan(0);
  expect(field.distance([5, 1.5, 0])).toBeLessThan(0);
  expect(field.distance([0, 6, 0])).toBeLessThan(0);
  const mesh = extractSurface(object.field, "interactive").mesh;
  expect(mesh.indices.length).toBeGreaterThan(0);
  expect([...mesh.positions].every(Number.isFinite)).toBe(true);
  expect([...mesh.normals].every(Number.isFinite)).toBe(true);
  const overhang = compileField(buildGeologyObject(terrain(), { ...cave, kind: "overhang" }).field);
  expect(overhang.distance([0, 1, 4])).toBeGreaterThan(0);
  expect(overhang.distance([0, 1, -4.5])).toBeLessThan(0);
  expect(overhang.distance([0, 7, 4])).toBeLessThan(0);
});

test("review reports slopes, walls, sightlines and bounds work on long paths", () => {
  const source = terrain();
  expect(reviewGeology(source)?.sightlineClear).toBe(true);
  source.geology.formations = [cave];
  expect(reviewGeology(source)?.collisionSamples).toBeGreaterThan(0);
  expect(reviewGeology(source)?.sightlineClear).toBe(false);
  source.geology.review.route = [
    [0, -10],
    [0, 10],
  ];
  expect(reviewGeology(source)?.collisionSamples).toBe(0);
  expect(reviewGeology(source)?.sightlineClear).toBe(true);
  source.geology.landforms = [
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
  expect(reviewGeology(source)?.steepSamples).toBeGreaterThan(0);
  source.geology.review.route = [
    [-1_000_000, 0],
    [1_000_000, 0],
  ];
  expect(reviewGeology(source)?.samples.length).toBe(512);
  source.geology.review.route = [
    [0, 0],
    [0, 0],
    [0, 0],
  ];
  expect(reviewGeology(source)?.samples.every((sample) => sample.position.every(Number.isFinite))).toBe(true);
});

test("compiled geological bounds and cached stencils preserve exact analytic samples", () => {
  const source = terrain();
  source.amplitude = 5;
  source.geology.erosion = { strength: 0.7, radius: 1.25, talusAngle: 25 };
  source.geology.strata = { thickness: 2.5, strength: 0.4 };
  source.geology.landforms = Array.from({ length: 24 }, (_, index) => ({
    id: `path-${index}`,
    kind: (["ridge", "drainage", "cliff"] as const)[index % 3],
    width: 0.5 + index,
    height: 2 + index,
    falloff: 0.25 + index / 4,
    points: Array.from({ length: 32 }, (_, point): [number, number] => [
      Math.sin(point * 1.31 + index) * 30,
      Math.cos(point * 0.43 + index) * 30,
    ]),
  }));
  source.geology.landforms[0].points = [
    [0, 0],
    [10, 0],
    [0, 0],
    [0, 0],
  ];
  source.geology.corridors = Array.from({ length: 16 }, (_, index) => ({
    id: `corridor-${index}`,
    halfWidth: 0.5 + index / 4,
    shoulder: 1 + index / 3,
    points: Array.from({ length: 8 }, (_, point): [number, number, number] => [
      index * 7 - 45,
      Math.sin(point + index),
      point * 9 - 32,
    ]),
  }));
  const sampler = createTerrainSampler(source);
  for (let i = 0; i < 200; i++) {
    const x = Math.sin(i * 3.71) * 60,
      z = Math.cos(i * 2.33) * 60;
    expect(sampler.height(x, z)).toBe(terrainHeight(source, x, z));
    expect(sampler.normal(x, z)).toEqual(terrainNormal(source, x, z));
  }
  for (const [x, z] of [
    [0, 0],
    [0, 0.5],
    [0, -0.5],
    [1e9, 1e9],
    [-1e9, -1e9],
  ]) {
    expect(sampler.height(x, z)).toBe(terrainHeight(source, x, z));
    expect(sampler.normal(x, z)).toEqual(terrainNormal(source, x, z));
  }
  source.geology.landforms[0].points[0] = [-100, 30];
  source.geology.landforms[0].height = 12;
  const edited = createTerrainSampler(source);
  expect(edited.height(0, 0)).toBe(terrainHeight(source, 0, 0));
});

test("protected corridor widths and grades survive geological resculpting and share render seams", () => {
  const source = terrain();
  source.geology.review.route = [
    [-10, 0],
    [0, 0],
    [10, 0],
  ];
  source.geology.review.maxSlope = 10;
  const corridor = createGeologyCorridor(source, (x) => x + 10, "trail");
  source.geology.corridors = [corridor];
  const rise = Math.tan((10 * Math.PI) / 180) * 10;
  expect(corridor.points[1][1]).toBeCloseTo(rise, 10);
  expect(corridor.points[2][1]).toBeCloseTo(rise * 2, 10);
  source.geology.landforms = [
    {
      id: "peak",
      kind: "ridge",
      points: [
        [0, -10],
        [0, 10],
      ],
      width: 6,
      height: 30,
      falloff: 1,
    },
  ];
  source.geology.erosion = { strength: 0.8, radius: 2, talusAngle: 5 };
  source.geology.strata = { strength: 1, thickness: 3 };
  expect(terrainHeight(source, 0, 0)).toBeCloseTo(rise, 10);
  expect(terrainHeight(source, 0, corridor.halfWidth)).toBeCloseTo(rise, 10);
  expect(terrainHeight(source, 0, corridor.halfWidth + corridor.shoulder)).toBeGreaterThan(10);
  const sampler = createTerrainSampler(source);
  for (let x = -10; x <= 10; x += 0.25)
    expect(sampler.height(x, 0)).toBeCloseTo((x + 10) * Math.tan((10 * Math.PI) / 180), 9);
  const left = generateTerrainPatch(source, -4, -4, 8, 16);
  const right = generateTerrainPatch(source, 4, -4, 8, 16);
  for (let row = 0; row <= 16; row++) {
    expect(left.positions[(row * 17 + 16) * 3 + 1]).toBe(right.positions[row * 17 * 3 + 1]);
    expect([...left.normals.slice((row * 17 + 16) * 3, (row * 17 + 16) * 3 + 3)]).toEqual([
      ...right.normals.slice(row * 17 * 3, row * 17 * 3 + 3),
    ]);
  }
  source.interventions = [
    { id: "crossing", kind: "flatten", center: [0, 0], radius: 2, strength: 1, targetHeight: 9 },
  ];
  expect(terrainHeight(source, 0, 0)).toBe(9);
});

test("graded corridor bends blend continuously and degenerate waypoints remain finite", () => {
  const source = terrain();
  source.geology.corridors = [
    {
      id: "bend",
      points: [
        [-10, -1, 0],
        [0, 0, 0],
        [0, 1, 10],
      ],
      halfWidth: 2,
      shoulder: 2,
    },
  ];
  expect(Math.abs(terrainHeight(source, -1, 1 - 1e-6) - terrainHeight(source, -1, 1 + 1e-6))).toBeLessThan(
    1e-5,
  );
  source.geology.corridors[0].points = [
    [0, 3, 0],
    [0, 3, 0],
  ];
  expect(terrainHeight(source, 0, 0)).toBe(3);
  expect(terrainHeight(source, 0.5, 0.5)).toBe(3);
  expect(terrainNormal(source, 0, 0)).toEqual([0, 1, 0]);
  source.geology.corridors.push({ ...source.geology.corridors[0] });
  expect(terrainGeologySchema.safeParse(source.geology).success).toBe(false);
});

test("stratified formations are deterministic, bounded and retain passable rotated openings", () => {
  const formation: GeologyFormation = {
    ...cave,
    heading: Math.PI / 2,
    rock: { seed: 17, layers: 6, fracture: 0.7 },
  };
  const object = buildGeologyObject(terrain(), formation);
  expect(documentSchema.safeParse(object).success).toBe(true);
  expect(buildGeologyObject(terrain(), formation)).toEqual(object);
  expect(
    buildGeologyObject(terrain(), { ...formation, rock: { seed: 18, layers: 6, fracture: 0.7 } }).field,
  ).not.toEqual(object.field);
  expect(object.field.nodes.length).toBeLessThanOrEqual(37);
  const field = compileField(object.field);
  for (const x of [-8, -4, 0, 4, 8]) expect(field.distance([x, 1.5, 0])).toBeGreaterThan(0);
  expect(field.distance([0, 6, 0])).toBeLessThan(0);
  expect(field.distance([0, 1.5, 5])).toBeLessThan(0);
  // Bedding must remain solid between layers; the earlier bevel construction
  // exposed narrow skylight leaks through the walls in the lookdev capture.
  for (let y = 0.2; y <= 4; y += 0.025) expect(field.distance([0, y, -4.8])).toBeLessThan(0);
  const mesh = extractSurface(object.field, "interactive").mesh;
  expect(mesh.indices.length).toBeGreaterThan(0);
  for (let i = 0; i < mesh.positions.length; i++) {
    expect(mesh.positions[i]).toBeGreaterThanOrEqual(object.field.bounds.min[i % 3]);
    expect(mesh.positions[i]).toBeLessThanOrEqual(object.field.bounds.max[i % 3]);
  }
});

test("body-width review catches a wall that a centerline route fits through", () => {
  const source = terrain();
  source.geology.formations = [cave];
  source.geology.review.route = [
    [3, -4],
    [3, 4],
  ];
  source.geology.review.clearance = 1;
  source.geology.review.bodyRadius = 0;
  expect(reviewGeology(source)?.collisionSamples).toBe(0);
  source.geology.review.bodyRadius = 1;
  expect(reviewGeology(source)?.collisionSamples).toBeGreaterThan(0);
  source.geology.formations[0].heading = Math.PI / 2;
  source.geology.review.route = [
    [-10, 0],
    [10, 0],
  ];
  source.geology.review.bodyRadius = 0.35;
  expect(reviewGeology(source)?.collisionSamples).toBe(0);
});
