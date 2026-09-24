import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { type WaterDefinition, waterAuthoringSchema } from "@wrela/model";
import { compileWaterPhases, waterPhaseFootprint } from "./phase";
import { queryWater, resolveWaterWaves, riverWaterMesh, waterMeshSpacing } from "./water";

function fixture(): WaterDefinition {
  const water = referenceProject().documents.find((d): d is WaterDefinition => d.kind === "water");
  if (!water) throw new Error("Missing water fixture");
  return {
    ...water,
    level: 1,
    waves: [{ amplitude: 0.4, wavelength: 6, speed: 1, direction: 0.6, phase: 0.2 }],
    flow: { velocity: [2, 1] },
  };
}

test("current advection agrees between analytic query and compiled/rendered phase", () => {
  const water = fixture(),
    time = 1.7,
    x = 2,
    z = 3;
  const wave = resolveWaterWaves(water)[0];
  expect(wave.speed).toBeCloseTo(1 + 2 * Math.cos(0.6) + Math.sin(0.6));
  const compiled = compileWaterPhases(water);
  const phase = waterPhaseFootprint(compiled, [x, z], time, [0, 0], [0, 0]).origin[0];
  const sample = queryWater(water, x, z, time);
  expect(sample.height).toBeCloseTo(water.level + wave.amplitude * Math.sin(phase), 10);
  expect(sample.velocity[0]).toBe(2);
  expect(sample.velocity[2]).toBe(1);
  const epsilon = 1e-5;
  const derivative =
    (queryWater(water, x, z, time + epsilon).height - queryWater(water, x, z, time - epsilon).height) /
    (2 * epsilon);
  expect(sample.velocity[1]).toBeCloseTo(derivative, 7);
  expect(water.waves[0].speed).toBe(1);
});

test("river queries bound buoyancy, follow channel flow, and soften shores", () => {
  const water = fixture();
  water.flow = {
    velocity: [3, 0],
    river: {
      points: [
        { position: [0, 0], width: 6, depth: 2 },
        { position: [0, 12], width: 6, depth: 4 },
      ],
      shoreWidth: 1,
      foam: 0.5,
    },
  };
  const center = queryWater(water, 0, 6, 0),
    edge = queryWater(water, 2.5, 6, 0);
  expect(center.velocity[0]).toBe(0);
  expect(center.velocity[2]).toBe(3);
  expect(center.depth).toBeCloseTo(3);
  expect(edge.shore).toBeCloseTo(0.5);
  expect(edge.velocity[2]).toBeCloseTo(1.5);
  expect(edge.depth).toBeLessThan(center.depth);
  expect(queryWater(water, 4, 6, 0).wet).toBe(false);
  expect(queryWater(water, 0, -1, 0).wet).toBe(false);
  expect(queryWater(water, 0, 13, 0).height).toBeLessThan(-1e6);
});

test("channel geometry uses joined banks, upward faces, and queryable vertices", () => {
  const water = fixture();
  water.flow = {
    velocity: [1, 0],
    river: {
      points: [
        { position: [0, 0], width: 4, depth: 2 },
        { position: [10, 0], width: 4, depth: 2 },
        { position: [10, 10], width: 4, depth: 2 },
      ],
      shoreWidth: 1,
      foam: 0.5,
    },
  };
  const mesh = riverWaterMesh(water, 1);
  if (!mesh) throw new Error("Missing channel mesh");
  for (let i = 0; i < mesh.positions.length; i += 3) {
    expect(queryWater(water, mesh.positions[i], mesh.positions[i + 2], 0).wet).toBe(true);
    expect(mesh.positions[i + 1]).toBe(water.level);
  }
  const [a, b, c] = Array.from(mesh.indices.slice(0, 3)).map((index) => [
    mesh.positions[index * 3],
    mesh.positions[index * 3 + 2],
  ]);
  expect((b[1] - a[1]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[1] - a[1])).toBeGreaterThan(0);
  expect(mesh.colors?.[0]).toBeGreaterThan(1);
  expect(waterMeshSpacing(mesh, 0)).toBeGreaterThan(1);
  expect(queryWater(water, 11.5, -1.5, 0).wet).toBe(true);
});

test("authoring rejects collapsed channel segments and keeps old water compatible", () => {
  expect(
    waterAuthoringSchema.safeParse({
      velocity: [0, 0],
      river: {
        points: [
          { position: [0, 0], width: 2, depth: 1 },
          { position: [0, 0], width: 2, depth: 1 },
        ],
      },
    }).success,
  ).toBe(false);
  const water = fixture();
  delete water.flow;
  expect(resolveWaterWaves(water)).toBe(water.waves);
  expect(riverWaterMesh(water)).toBeUndefined();
});

test("river review rejects folded and crossing banks before emitting invalid geometry", async () => {
  const { reviewRiverChannel } = await import("./river-review");
  const water = fixture();
  const river = (positions: [number, number][], width = 2) => ({
    points: positions.map((position) => ({ position, width, depth: 1 })),
    shoreWidth: 0.2,
    foam: 0.1,
  });
  const valid = river([
    [0, 0],
    [10, 0],
    [10, 10],
  ]);
  expect(reviewRiverChannel(valid).filter((d) => d.severity === "error")).toEqual([]);
  const folded = river(
    [
      [0, 0],
      [1, 0],
      [1, 1],
    ],
    8,
  );
  expect(reviewRiverChannel(folded).some((d) => d.code === "folded-bank")).toBe(true);
  expect(() => riverWaterMesh({ ...water, flow: { velocity: [1, 0], river: folded } })).toThrow("folds");
  const crossing = river([
    [-10, -10],
    [10, 10],
    [-10, 10],
    [10, -10],
  ]);
  expect(reviewRiverChannel(crossing).some((d) => d.code === "crossing-channel")).toBe(true);
  const { createEnvironmentLookdev } = await import("@wrela/examples/environment-lookdev");
  const creek = createEnvironmentLookdev().documents.find((d) => d.kind === "water");
  if (creek?.kind !== "water" || !creek.flow?.river) throw new Error("Creek missing");
  expect(reviewRiverChannel(creek.flow.river).filter((d) => d.severity === "error")).toEqual([]);
});
