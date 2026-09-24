import { expect, test } from "bun:test";
import type { EvaluatedScene, IndirectLightingField } from "@wrela/model";

import { indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import { packIndirectLighting, withIndirectReceivers } from "./indirect";

function fixtureField(): IndirectLightingField {
  return {
    key: "test",
    revision: 1,
    origin: [1e7 + 0.25, 2, 3],
    spacing: [1, 1, 1],
    dimensions: [2, 2, 2],
    data: new Float32Array(480),
    completedProbes: 0,
    totalProbes: 8,
    report: {
      status: "building",
      triangles: 0,
      rays: 0,
      source: "constant-sky-and-directional-sun",
      bounces: 1,
      excluded: [],
      buildMs: 0,
      maxSliceMs: 0,
    },
  };
}

test("indirect field packing rebases probe coordinates and disables missing transport", () => {
  const field = fixtureField();
  const packed = packIndirectLighting(field, [1e7, 0, 0]);
  expect([...packed.slice(0, 4)]).toEqual([0.25, 2, 3, 1]);
  expect(packed.length).toBe(496);
  expect(packIndirectLighting()[3]).toBe(0);
  expect(() => packIndirectLighting({ ...field, dimensions: [33, 33, 33] })).toThrow();
  field.transfer = new Float32Array(8 * 360).fill(0.25);
  field.reflections = {
    data: new Float32Array(8 * 36).fill(0.75),
    transfer: new Float32Array(8 * 360).fill(0.5),
  };
  field.visibility = { nodes: new Float32Array(8), triangles: new Float32Array(12) };
  const complete = packIndirectLighting(field, [1e7, 0, 0]);
  const partial = packIndirectLighting(field, [1e7, 0, 0], false);
  expect(complete[3]).toBe(6);
  expect([...partial]).toEqual([...complete.subarray(0, partial.length)]);
  expect(complete[12] * 4).toBe(16 + field.data.length + field.reflections.data.length);
  expect(complete[complete[7] * 4]).toBe(0.25);
  expect(complete[complete[7] * 4 + field.transfer.length]).toBe(0.5);
  expect(() =>
    packIndirectLighting({ ...field, reflections: { ...field.reflections, data: new Float32Array(2) } }),
  ).toThrow();
});

test("surface visibility proof requires the exact source mesh, range and absolute pose", () => {
  const surface = indirectBoxFixture().surfaces[0];
  const field: IndirectLightingField = {
    ...fixtureField(),
    receivers: [
      {
        id: surface.id,
        positions: surface.mesh.positions,
        indices: surface.mesh.indices,
        matrix: Array.from(surface.matrix),
        start: 0,
        count: surface.mesh.indices.length,
      },
    ],
  };
  const scene = { surfaces: [surface], indirectLighting: field } as EvaluatedScene;
  expect(withIndirectReceivers(scene).surfaces[0].staticIndirectReceiver).toBe(true);
  for (const changed of [
    { ...surface, lightingMobility: "dynamic" as const },
    { ...surface, mesh: { ...surface.mesh, positions: surface.mesh.positions.slice() } },
    { ...surface, drawRange: { start: 0, count: 3 } },
    { ...surface, wind: 0.2 },
    { ...surface, matrix: Float32Array.from(surface.matrix, (v, i) => (i === 12 ? v + 1 : v)) },
  ])
    expect(withIndirectReceivers({ ...scene, surfaces: [changed] }).surfaces[0].staticIndirectReceiver).toBe(
      false,
    );
  const rebased = {
    ...surface,
    matrix: Float32Array.from(surface.matrix, (v, i) => (i === 12 ? v - 16 : v)),
  };
  expect(
    withIndirectReceivers({ ...scene, origin: [16, 0, 0], surfaces: [rebased] }).surfaces[0]
      .staticIndirectReceiver,
  ).toBe(true);
  expect(
    withIndirectReceivers({
      ...scene,
      indirectLighting: undefined,
      surfaces: [{ ...surface, staticIndirectReceiver: true }],
    }).surfaces[0].staticIndirectReceiver,
  ).toBe(false);
});
