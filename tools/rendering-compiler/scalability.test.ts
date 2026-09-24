import { expect, test } from "bun:test";
import { type EvaluatedScene, identityMatrix } from "@wrela/model";

import { applyWorkload, SCALABILITY_WORKLOADS, workloadInventory } from "./scalability";

const material = {
  color: [1, 1, 1] as [number, number, number],
  secondary: [1, 1, 1] as [number, number, number],
  roughness: 0.5,
  metallic: 0,
  pattern: 0,
  scale: 1,
  normalStrength: 0,
};
const mesh = {
  positions: new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]),
  normals: new Float32Array(9),
  indices: new Uint32Array([0, 1, 2]),
  bounds: { min: [0, 0, 0] as [number, number, number], max: [1, 1, 0] as [number, number, number] },
};
const scene = {
  surfaces: [
    { id: "tree", source: "tree", wind: 1, mesh, material, matrix: identityMatrix() },
    {
      id: "actor",
      source: "actor",
      mesh,
      material,
      matrix: identityMatrix(),
      skin: { matrices: identityMatrix(), jointIndices: new Uint16Array(12), weights: new Float32Array(12) },
    },
  ],
  camera: { position: [5, 3, 8], target: [0, 1, 0], fov: 50 },
  environment: { pointLights: [] },
  time: 0,
  mode: "beauty",
  grid: false,
} as unknown as EvaluatedScene;
test("scalability conditions preserve source data, unique identities and requested workload counts", () => {
  for (const workload of SCALABILITY_WORKLOADS) {
    const output = applyWorkload(scene, workload),
      inventory = workloadInventory(output);
    expect(new Set(output.surfaces.map((s) => s.id)).size).toBe(output.surfaces.length);
    expect(inventory.foliageSurfaces).toBe(workload.foliage ?? 1);
    expect(inventory.skinnedSurfaces).toBe(workload.characters ?? 1);
    expect(inventory.particles).toBe(workload.particles ?? 0);
    expect(inventory.pointLights).toBe(workload.lights ?? 0);
  }
  expect(scene.surfaces.length).toBe(2);
  expect(scene.surfaces[0].matrix).toEqual(identityMatrix());
  expect(scene.surfaces[0].material.layers).toBeUndefined();
  expect(() => applyWorkload(scene, { name: "invalid", lights: 9 })).toThrow();
  const a = applyWorkload(scene, { name: "debris", particles: 2 });
  const b = applyWorkload({ ...scene, time: 1 }, { name: "debris", particles: 2 });
  expect(a.surfaces.at(-1)?.matrix).not.toEqual(b.surfaces.at(-1)?.matrix);
});
