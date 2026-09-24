import { expect, test } from "bun:test";
import {
  creatureSchema,
  creatureSculptSchema,
  evaluateCreatureSculpt,
  type MeshData,
  sculptSupport,
} from "@wrela/model";

import { sculptLegacyCreatureGeometry } from "./creature";
import { refineCreatureSculptSurface } from "./creature-refinement";

const stroke = creatureSculptSchema.parse({
  id: "crease",
  region: "face",
  center: [0, 0, 0],
  radius: 1,
  displacement: [0, 0, 0.1],
  strength: 1,
  falloff: 2,
  support: { radii: [0.15, 0.5, 0.2], rotation: [0, 0, 0] },
  detail: { maxEdgeLength: 0.1, passes: 6 },
});
const creature = creatureSchema.parse({
  schemaVersion: 1,
  regions: [
    {
      id: "face",
      name: "Face",
      nodeIds: ["skin"],
      jointIds: [],
      frame: { position: [0, 0, 0], rotation: [0, 0, 0] },
      extent: [2, 2, 2],
    },
  ],
  sculpts: [stroke],
});
const mesh: MeshData = {
  positions: new Float32Array([-1, -1, 0, 1, -1, 0, 1, 1, 0, -1, 1, 0, 4, 0, 0, 5, 0, 0, 4, 1, 0]),
  normals: new Float32Array(Array.from({ length: 7 }, () => [0, 0, 1]).flat()),
  indices: new Uint32Array([0, 1, 2, 0, 2, 3, 4, 5, 6]),
  sourceIds: Array(7).fill("skin"),
  bounds: { min: [-1, -1, 0], max: [5, 1, 0] },
  materialGroups: [
    { material: "skin", start: 0, count: 6 },
    { material: "other", start: 6, count: 3 },
  ],
};
test("oriented curve support is compact, smooth and mirrored as an entire field", () => {
  const curve = {
    ...stroke,
    path: [
      [0.3, -0.4, 0],
      [0.3, 0.4, 0],
    ] as [number, number, number][],
    mirror: true,
  };
  expect(evaluateCreatureSculpt([curve], "face", [0.3, 0, 0])[2]).toBeCloseTo(0.1);
  expect(evaluateCreatureSculpt([curve], "face", [-0.3, 0, 0])[2]).toBeCloseTo(0.1);
  expect(evaluateCreatureSculpt([curve], "face", [0, 0, 0])).toEqual([0, 0, 0]);
  expect(
    sculptSupport(
      { ...stroke, support: { radii: [0.1, 0.7, 0.2], rotation: [0, 0, Math.PI / 2] } },
      [0.5, 0, 0],
    ).weight,
  ).toBeGreaterThan(0);
  expect(sculptSupport(stroke, [0.5, 0, 0]).weight).toBe(0);
});
test("flatten displaces toward a plane with a physical cap; legacy push remains exact", () => {
  const flat = { ...stroke, mode: "flatten" as const };
  const p: [number, number, number] = [0, 0, 0.05];
  expect(evaluateCreatureSculpt([flat], "face", p)[2]).toBeLessThan(p[2]);
  expect(evaluateCreatureSculpt([flat], "face", p)[2]).toBeGreaterThanOrEqual(0);
  expect(
    evaluateCreatureSculpt([{ ...stroke, support: undefined, detail: undefined }], "face", [0, 0, 0])[2],
  ).toBe(0.1);
});
test("local refinement reveals a field missed by all original vertices without refining distant triangles", () => {
  expect(sculptLegacyCreatureGeometry(mesh, creature).positions).toEqual(mesh.positions);
  const refined = refineCreatureSculptSurface(mesh, creature);
  expect(refined.mesh.positions.length).toBeGreaterThan(mesh.positions.length);
  const sculpted = sculptLegacyCreatureGeometry(refined.mesh, creature);
  expect(sculpted.bounds.max[2]).toBeCloseTo(0.1);
  expect(refined.mesh.materialGroups?.at(-1)?.count).toBe(3);
  expect(mesh.positions.length).toBe(21);
  expect(refined.diagnostics).toEqual([]);
  const usage = new Map<string, number>();
  for (let i = 0; i < refined.mesh.indices.length - 3; i += 3)
    for (let j = 0; j < 3; j++) {
      const a = refined.mesh.indices[i + j],
        b = refined.mesh.indices[i + ((j + 1) % 3)],
        key = [a, b].sort((x, y) => x - y).join(":");
      usage.set(key, (usage.get(key) ?? 0) + 1);
    }
  for (const [edge, count] of usage)
    if (count === 1) {
      const [a, b] = edge.split(":").map(Number);
      const p = refined.mesh.positions;
      expect(
        (Math.abs(p[a * 3]) === 1 && p[a * 3] === p[b * 3]) ||
          (Math.abs(p[a * 3 + 1]) === 1 && p[a * 3 + 1] === p[b * 3 + 1]),
      ).toBe(true);
    }
});
test("refinement reports exhausted budgets and never mutates or partially splits a shared edge", () => {
  const result = refineCreatureSculptSurface(mesh, creature, 8);
  expect(result.mesh.indices).toEqual(mesh.indices);
  expect(result.diagnostics[0]?.code).toBe("creature-sculpt-detail");
  expect(result.mesh.positions.length / 3).toBeLessThanOrEqual(8);
});

test("a scoped facial correction leaves nearby eyes in the same region untouched", () => {
  const scoped = creatureSchema.parse({
    ...creature,
    regions: [{ ...creature.regions[0], nodeIds: ["skin", "eye"] }],
    sculpts: [{ ...stroke, nodeIds: ["skin"] }],
  });
  const eye = { ...mesh, sourceIds: Array(7).fill("eye") };
  expect(refineCreatureSculptSurface(eye, scoped).mesh.indices).toEqual(eye.indices);
  expect(sculptLegacyCreatureGeometry(eye, scoped).positions).toEqual(eye.positions);
  const skin = refineCreatureSculptSurface(mesh, scoped).mesh;
  expect(sculptLegacyCreatureGeometry(skin, scoped).bounds.max[2]).toBeCloseTo(0.1);
});
