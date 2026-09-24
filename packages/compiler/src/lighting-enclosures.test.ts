import { expect, test } from "bun:test";
import { indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import { compileIndirectGeometry } from "./indirect-query";
import { insideLightingEnclosure, lightingEnclosureSteps } from "./lighting-enclosures";
import { radianceReceiverMesh, radianceFlatNormals } from "./radiance-receivers";

function finish<T>(steps: Generator<void, T>): T {
  let step = steps.next();
  while (!step.done) step = steps.next();
  return step.value;
}

test("only closed convex shells certify enclosure; windows and tiny cracks remain open", () => {
  for (const kind of ["closed", "open", "crack"] as const) {
    const f = indirectBoxFixture({ ceiling: true, front: kind !== "open", occluder: false });
    if (kind === "crack") f.surfaces.find((s) => s.id === "front-wall")!.matrix[14] += 0.00001;
    const geometry = compileIndirectGeometry(f.surfaces),
      regions = finish(lightingEnclosureSteps(geometry));
    expect(insideLightingEnclosure(geometry, regions, [0, 1, 0]) > 0).toBe(kind === "closed");
    expect(insideLightingEnclosure(geometry, regions, [0, 1, 2])).toBe(0);
  }
});

test("receiver refinement preserves affine attributes, bounds and original source geometry", () => {
  const s = indirectBoxFixture({ occluder: false }).surfaces[0];
  s.mesh.materialCoordinates = Float32Array.from(s.mesh.positions, (v, i) => v * 2 + (i % 3));
  s.mesh.colors = Float32Array.from(s.mesh.positions, (v) => v * 0.1 + 0.5);
  s.mesh.sourceIds = Array.from({ length: s.mesh.positions.length / 3 }, () => "floor");
  const original = s.mesh,
    result = finish(radianceReceiverMesh(s, 1000));
  expect(result.sources).toBeDefined();
  expect(result.mesh.positions.length).toBeGreaterThan(original.positions.length);
  expect(result.mesh.bounds).toBe(original.bounds);
  expect(s.mesh).toBe(original);
  for (let i = 0; i < result.mesh.positions.length; i++) {
    expect(result.mesh.positions[i % 3 === 1 ? i : i - (i % 3) + 1]).toBe(0);
    expect(result.mesh.materialCoordinates![i]).toBeCloseTo(result.mesh.positions[i] * 2 + (i % 3), 5);
    expect(result.mesh.colors![i]).toBeCloseTo(result.mesh.positions[i] * 0.1 + 0.5, 5);
  }
  expect(result.mesh.sourceIds?.every((id) => id === "floor")).toBe(true);
  expect(finish(radianceReceiverMesh(s, 1)).mesh).toBe(original);
  s.mesh.sourceIds[1] = "other-part";
  expect(finish(radianceReceiverMesh(s, 1000)).mesh).toBe(original);
});

test("denser planar receivers share vertices and certify both camera-facing sides only for geometric normals", () => {
  const s = indirectBoxFixture().surfaces[0];
  const refined = finish(radianceReceiverMesh(s, 10000)).mesh;
  expect(refined.positions.length / 3).toBeLessThan(refined.indices.length / 2);
  expect(refined.indices.every((i) => i < refined.positions.length / 3)).toBe(true);
  expect(radianceFlatNormals(refined)).toBe(true);
  const artistic = { ...refined, normals: refined.normals.slice() };
  artistic.normals[0] += 0.1;
  expect(radianceFlatNormals(artistic)).toBe(false);
});
